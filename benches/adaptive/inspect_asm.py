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
        selected=(name.startswith('netindex_adaptive_core::study_flat_btree::Tree<K>::walk_')
            or name=='netindex_adaptive_core::study_adaptive::choose'
            or name.startswith('netindex_adaptive_core::reader::Reader<B>::study_plan_'))
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
                      sse_compares=sum(op.startswith('pcmp') for op,_ in instructions),
                      dword_memory_operands=sum('DWORD PTR' in operands and 'QWORD PTR' not in operands and 'XMMWORD' not in operands and 'YMMWORD' not in operands for _,operands in instructions),
                      xmm_operand_instructions=sum('xmm' in operands for _,operands in instructions),
                      ymm_operand_instructions=sum('ymm' in operands for _,operands in instructions),
                      calls_before_first_return=sum(op=='call' for op,_ in instructions[:next((i for i,(op,_) in enumerate(instructions) if op=='ret'),len(instructions))]))
        kernels.append(dict(symbol=symbol, symbol_bytes=int(symbol.split()[1],16),
            instruction_counts=counts,
            body=body.splitlines(),
            rsp_memory_operand_instructions=sum('[rsp' in operands for _, operands in instructions),
            prologue=body.splitlines()[:8]))
    assert sum('::walk_count' in k['symbol'] for k in kernels)==1
    assert sum('::walk_vector' in k['symbol'] for k in kernels)==1
    assert any('study_adaptive::choose' in k['symbol'] for k in kernels)
    assert any('study_plan_ips' in k['symbol'] for k in kernels)
    dispatch=[]
    for index,header in enumerate(headers):
        if header[2]!='netindex_adaptive_study::execute':
            continue
        body=text[header.end():headers[index+1].start()]
        symbol=next(line for line in symbols if line.startswith(header[1]+' '))
        lines=body.splitlines()
        selected=[i for i,line in enumerate(lines) if re.search(r'\bDWORD PTR \[[^]]*\*4',line)]
        dispatch.append(dict(symbol=symbol,symbol_bytes=int(symbol.split()[1],16),
            body_sha256=hashlib.sha256(body.encode()).hexdigest(),
            u32_scaled_access_sites=len(selected),
            examples=[lines[max(0,i-2):i+3] for i in selected[:8]],
            scope='Generic execute contains multiple timed/correctness variants and inlined native-IP searches. Examples show u32 scaled accesses, which can also belong to ASN paths; they do not isolate the IPv4 loop or establish width-only performance. Full body retained in hashed local disassembly.'))
    assert len(dispatch)==5 and all(d['u32_scaled_access_sites']>0 for d in dispatch)
    rows.append(dict(profile=profile, executable_sha256=result['executable_sha256'][profile],
        disassembly_sha256=hashlib.sha256(raw).hexdigest(), kernels=kernels, inlined_dispatch=dispatch))
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
    conclusion='Bounded policy chooser, request planners, generic execute summaries with inlined IP searches and admitted ASN scalar/AVX2 walks. Other caches use native-width sorted partition search in generic readers. Static counts establish emitted code, not runtime cost or causal attribution; budget/admission and policy traces are independently reconstructed in verification.',
    counter_probe=counters, rows=rows),indent=2)+'\n')
print(destination)
