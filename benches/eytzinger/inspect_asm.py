"""Inspect linked AVX2 kernels: PRIMARY WORK OUTPUT. Full dumps stay local."""
import hashlib
import json
import pathlib
import re
import subprocess
import sys

primary, work, destination = map(pathlib.Path, sys.argv[1:])
result = json.loads(primary.read_text())
rows = []
for profile in ['default', 'simd']:
    run = next(r for r in result['runs'] if r['profile'] == profile)
    executable = pathlib.Path(run['arguments'][0])
    assert hashlib.sha256(executable.read_bytes()).hexdigest() == result['executable_sha256'][profile]
    raw = subprocess.check_output(['objdump', '-d', '-C', '-M', 'intel', '--no-show-raw-insn', str(executable)])
    (work / f'disassembly-{profile}.txt').write_bytes(raw)
    text = raw.decode()
    headers = list(re.finditer(r'(?m)^([0-9a-f]+) <(.*)>:\n', text))
    symbols = subprocess.check_output(['nm', '-S', '-C', str(executable)], text=True).splitlines()
    kernels = []
    for index, header in enumerate(headers):
        if header[2] != 'netindex_eytzinger_core::study_eytzinger::Tree<K>::bound':
            continue
        body = text[header.end():headers[index+1].start()]
        symbol = next(line for line in symbols if line.startswith(header[1]+' '))
        instructions = re.findall(r'(?m)^\s*[0-9a-f]+:\s+(\w+)\s*(.*)', body)
        counts = dict(conditional_moves=sum(op.startswith('cmov') for op,_ in instructions),
                      set_condition=sum(op.startswith('set') for op,_ in instructions),
                      conditional_jumps=sum(op.startswith('j') and op!='jmp' for op,_ in instructions))
        kernels.append(dict(symbol=symbol, symbol_bytes=int(symbol.split()[1],16),
            instruction_counts=counts,
            body=body.splitlines(),
            rsp_memory_operand_instructions=sum('[rsp' in operands for _, operands in instructions),
            prologue=body.splitlines()[:8]))
    assert len(kernels) == 2
    rows.append(dict(profile=profile, executable_sha256=result['executable_sha256'][profile],
        disassembly_sha256=hashlib.sha256(raw).hexdigest(), kernels=kernels))
counter_command = ['perf', 'stat', '-e', 'cycles,instructions,branches,branch-misses,cache-misses', '--', '/bin/true']
try:
    probe = subprocess.run(counter_command, capture_output=True, text=True, timeout=10)
    counters = dict(command=counter_command, exit_code=probe.returncode, stdout=probe.stdout, stderr=probe.stderr,
                    status='available probe only; no workload counters measured' if probe.returncode == 0 else 'unavailable')
except FileNotFoundError:
    counters = dict(command=counter_command, status='unavailable', reason='perf executable absent')
destination.write_text(json.dumps(dict(date='2026-11-04', primary_sha256=hashlib.sha256(primary.read_bytes()).hexdigest(),
    tool=subprocess.check_output(['objdump','--version'],text=True).splitlines()[0],
    inspection_source_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
    conclusion='Two key-width specializations per executable implement sorted partition search, candidate-tracking heap traversal and arithmetic child/path-recovery traversal in a shared runtime-dispatch method. The receipt retains complete bounded helper bodies and conditional move/set/jump counts; these whole-function counts include all three paths and are not runtime execution/counter data. Source arithmetic does not imply a branch-free compiled algorithm. No prefetch instructions were introduced. Inspect query-comparison direction, child selection, final original-rank lookup and bounds-failure paths before any adoption.',
    counter_probe=counters, rows=rows),indent=2)+'\n')
print(destination)
