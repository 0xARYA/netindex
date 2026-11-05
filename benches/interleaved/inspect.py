"""Inspect selected timed executor addresses: PRIMARY WORK DEFAULT_HEX SIMD_HEX OUTPUT."""
import hashlib
import json
import pathlib
import re
import subprocess
import sys

primary, work, default_address, simd_address, destination = sys.argv[1:]
primary, work, destination = map(pathlib.Path, [primary, work, destination])
result = json.loads(primary.read_text())
rows = []
for profile, address in [('default', default_address), ('simd', simd_address)]:
    run = next(r for r in result['runs'] if r['profile'] == profile)
    executable = pathlib.Path(run['arguments'][0])
    assert hashlib.sha256(executable.read_bytes()).hexdigest() == result['executable_sha256'][profile]
    raw = subprocess.check_output(['objdump', '-d', '-C', '-M', 'intel', '--no-show-raw-insn', str(executable)])
    (work / f'disassembly-{profile}.txt').write_bytes(raw)
    text = raw.decode()
    headers = list(re.finditer(r'(?m)^([0-9a-f]+) <(.*)>:\n', text))
    index = next(i for i, h in enumerate(headers) if int(h[1], 16) == int(address, 16))
    header = headers[index]
    assert header[2] == 'netindex_interleaved_study::execute'
    body = text[header.end():headers[index+1].start()]
    symbols = subprocess.check_output(['nm', '-S', '-C', str(executable)], text=True).splitlines()
    symbol = next(line for line in symbols if line.startswith(header[1] + ' '))
    instructions = re.findall(r'(?m)^\s*[0-9a-f]+:\s+(\w+)\s*(.*)', body)
    prologue = body.splitlines()[:15]
    reserve = sum(int(value, 16) for value in re.findall(r'sub\s+rsp,0x([0-9a-f]+)', '\n'.join(prologue)))
    calls = [line.strip() for line in body.splitlines() if 'call' in line and ('packed' in line or 'search' in line)]
    rows.append(dict(profile=profile, executable_sha256=result['executable_sha256'][profile],
                     disassembly_sha256=hashlib.sha256(raw).hexdigest(), symbol=symbol,
                     symbol_bytes=int(symbol.split()[1], 16), stack_reservation_bytes=reserve,
                     prologue=prologue, packed_search_calls=calls,
                     conditional_move_instructions=sum(op.startswith('cmov') for op, _ in instructions),
                     conditional_jump_instructions=sum(op.startswith('j') and op != 'jmp' for op, _ in instructions),
                     stack_addressing_instructions=sum('[rsp' in operands for _, operands in instructions)))
destination.write_text(json.dumps(dict(date='2026-11-04', primary_sha256=hashlib.sha256(primary.read_bytes()).hexdigest(),
    tool=subprocess.check_output(['objdump', '--version'], text=True).splitlines()[0],
    inspection_source_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
    role='Addresses selected by inspecting main: call inside timed repetition loop, with hit-count/checksum accumulator pointers, between Instant now and elapsed calls. All lane variants share this dispatch specialization; it is not the direct scalar library method alone.',
    conclusions='The timed dispatch specialization is about 25 KiB and reserves 6920 bytes of stack before saved registers. Fixed arrays/state and ordinary packed-tail calls remain visible. Conditional moves/jumps and stack-addressing counts describe the whole static dispatcher, including unused variants; they are not runtime counters or spill/hotspot measurements. Scalar baseline shares the dispatcher frame once per entire corpus call. This supports investigating lower-overhead family-specific kernels; it does not prove the cause of gains/regressions or establish hardware memory-level parallelism.',
    rows=rows), indent=2)+'\n')
print(destination)
