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
        name=header[2]
        selected=(name.startswith('netindex_traversal_core::study_eytzinger::Tree<K>::walk')
            or name.startswith('netindex_traversal_core::study_flat_btree::Tree<K>::walk_')
            or name.startswith('netindex_traversal_core::study_flat_btree::Tree<K>::bound')
            or ' as netindex_traversal_core::study_flat_btree::Key>::' in name
            or name in ['netindex_traversal_core::study_flat_btree::rank32','netindex_traversal_core::study_flat_btree::rank128'])
        if not selected:
            continue
        body = text[header.end():headers[index+1].start()]
        symbol = next(line for line in symbols if line.startswith(header[1]+' '))
        instructions = re.findall(r'(?m)^\s*[0-9a-f]+:\s+(\w+)\s*(.*)', body)
        counts = dict(conditional_moves=sum(op.startswith('cmov') for op,_ in instructions),
                      set_condition=sum(op.startswith('set') for op,_ in instructions),
                      conditional_jumps=sum(op.startswith('j') and op!='jmp' for op,_ in instructions),
                      prefetch_t0=sum(op=='prefetcht0' for op,_ in instructions),
                      vector_compares=sum(op.startswith('vpcmp') for op,_ in instructions),
                      calls_before_first_return=sum(op=='call' for op,_ in instructions[:next((i for i,(op,_) in enumerate(instructions) if op=='ret'),len(instructions))]))
        kernels.append(dict(symbol=symbol, symbol_bytes=int(symbol.split()[1],16),
            instruction_counts=counts,
            body=body.splitlines(),
            rsp_memory_operand_instructions=sum('[rsp' in operands for _, operands in instructions),
            prologue=body.splitlines()[:8]))
    assert sum('study_eytzinger::Tree<K>::walk' in k['symbol'] for k in kernels) == 16
    assert sum('::walk_count' in k['symbol'] for k in kernels) == 2
    assert sum('::walk_vector' in k['symbol'] for k in kernels) == 2
    eyt=[k for k in kernels if 'study_eytzinger::Tree<K>::walk' in k['symbol']]
    assert sum(k['instruction_counts']['prefetch_t0']>0 for k in eyt)==8
    specialized=[k for k in kernels if '::walk_vector' in k['symbol'] or '::walk_count' in k['symbol']]
    assert all(k['instruction_counts']['calls_before_first_return']==0 for k in specialized)
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
    conclusion='Complete eight-factor/two-width Eytzinger walks and specialized B+ count/vector walks, legacy B+ dispatch and wrappers. Prefetch/conditional counts are static, not runtime counter data. Inspect arithmetic child calculation, candidate branches, guarded prefetches, direct AVX2 inlining and per-query detection before adoption. ISA dispatch/inlining changes form one combined B+ intervention.',
    counter_probe=counters, rows=rows),indent=2)+'\n')
print(destination)
