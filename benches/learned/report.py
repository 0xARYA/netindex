"""Render retained learned-fence evidence: PRIMARY REPEAT ASM OUTPUT."""
import json
import pathlib
import statistics
import sys

primary, repeat, assembly, output = map(pathlib.Path,sys.argv[1:])
main,fresh,asm=[json.loads(p.read_text()) for p in [primary,repeat,assembly]]
variants=[0,108,700,701,702]


def median(run,traffic,variant):
    return statistics.median(r['ns_per_query'] for r in run['rows'] if r['phase']=='measure' and r['traffic']==traffic and r['lanes']==variant)


def cache(run):
    return next(r for r in run['rows'] if r['phase']=='cache')


def table(runs):
    lines=['| Family / shape | Build | Error | Traffic | 0 | 108 | 700 | 701 | 702 |',
           '| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |']
    for run in runs:
        for check in [r for r in run['rows'] if r['phase']=='checked']:
            traffic=check['traffic']
            lines.append('| '+' | '.join([run['scenario'],run['profile'],str(run['epsilon']),traffic]+[f'{median(run,traffic,v):.1f}' for v in variants])+' |')
    return lines


lines=['# Learned-fence results - 2026-11-04','',
    'Decision: retain guarded learned prediction as a workload-specific research',
    'candidate. No production default, public API, encoding or release change.',
    'Large evenly spaced inputs are favorable; small nonlinear cases and hot',
    'traffic prevent a universal adoption claim. This is an independent Rust',
    'corridor prototype inspired by RadixSpline, not an upstream port/benchmark.','',
    '[Protocol/reproduction/reference](README.md), [primary matrix](../results/2026-11-04-learned.json),',
    '[fresh repeats](../results/2026-11-04-learned-repeat.json),',
    '[assembly/counters](../results/2026-11-04-learned-asm.json),',
    '[integrity/fixture reproduction](../results/2026-11-04-learned-verification.json).','',
    '## Scope and correctness','',
    '144 primary processes plus 66 fresh repeats yield 5,280 timings. Error',
    'settings 4/32/128 run in separate processes, default/SIMD builds on WSL CPU 20.',
    'Portable release/codegen-units=1, no PGO/LTO/custom target flags. Both builds',
    'detect AVX2 at runtime. Four forward/reverse trials and a warmup precede each',
    'measurement. Every table entry is median warm aggregate ns/query, not latency',
    'percentiles or statistical significance. Process/host drift remains relevant.','',
    '0 ordinary; 108 prior cross-query AVX2; 700 native sorted-key cache; 701 learned',
    'binary segment routing; 702 learned radix routing. Learned routes search a',
    'window of at most 2*error+5 keys, then validate global neighbors and fall back',
    'to full native search if necessary. Requested error is not a universal bound',
    'for unseen gaps/duplicate jumps/floating rounding; global checks prove exact',
    'rank. Single-segment models bypass segment routing and allocate no radix.',
    'Correction remains scalar/native; no new explicit SIMD kernel is added.',
    'Estimate/search retain non-inlined boundaries for linked-body inspection;',
    'call/result-record costs are timed. Inlining/specialization is not an isolated',
    'factor here. Both native-control/learned search return rank/window/recovery',
    'state; the timed reader uses rank, with no mutable per-query counters.','',
    'Every original assertion and full independent original-input query oracle',
    'passes, including targets/IDs, duplicate multiplicity, borrowed payload bytes/',
    'pointers, partial/empty/mixed batches, broad routes and typed callback-error',
    'prefixes. Forced rank-zero predictions/scalar paths and 15 generated-core',
    'tests cover recovery, NaN slopes, unsigned limits, gaps, duplicates, sortedness,',
    'allocation budgets and preparation identity. Fresh preparation reproduces',
    'the full generated tree. Eight new curved/clustered files reproduce byte-for-',
    'byte. Header-derived eligibility, capacities/radix dimensions, probes, all',
    'timing coordinates/result checksums and receipt/source hashes agree.','',
    '## Fixed error 4 qualification on 20M inputs','',
    'For each row, ranges span primary/repeat and default/SIMD processes. Keep',
    'error 4 and route 702 fixed across this comparison; these are not post-hoc',
    'fastest learned variants. The best-control comparator is the minimum median',
    'of 0/108/700 in the same process and is optimistic rather than deployable.',
    'Ratios above 1 favor route 702; ratios below 1 indicate regressions.','',
    '| Family | Traffic | Ordinary /702 range | Best fixed control /702 range |',
    '| --- | --- | ---: | ---: |']
large=[r for r in main['runs']+fresh['runs'] if r['records']==20_000_000]
for family in ['v4','v6','asn']:
    selected=[r for r in large if r['scenario']==family and r['epsilon']==4]
    for traffic in ['uniform','hot']:
        speed=[median(r,traffic,0)/median(r,traffic,702) for r in selected]
        fixed=[min(median(r,traffic,v) for v in variants[:3])/median(r,traffic,702) for r in selected]
        lines.append(f'| {family} | {traffic} | {min(speed):.2f}-{max(speed):.2f}x | {min(fixed):.2f}-{max(fixed):.2f}x |')
lines+=['','The large original inputs have uniformly spaced keys and each uses one',
    'segment. The nonlinear controls contain only 256 packed fences per file, so',
    'they qualify small-array overhead and shape behavior, not large nonlinear',
    'indexes or provider traffic. Their midpoint gaps are included in timings.',
    'Wider-than-u64 sparse IPv6 and overlapping/interval shapes keep ordinary',
    'fallback. Those rows do not measure learned acceleration.','',
    '## Additional cache and startup','',
    'One native key array/model per admitted section, shared by all comparators.',
    'The 128 MiB bound covers vector element capacities (keys, 40-byte segments',
    'including spare capacity, u32 radix entries), not mapping/validation, existing',
    'reader directory, objects, allocator overhead or RSS. Ordinary controls also',
    'retain these arrays. Build time follows ordinary validation and precedes',
    'queries; it does not isolate model fitting from native-key extraction.',
    'Zero-copy/strided correction is not implemented or measured.','',
    '| 20M family | Error | Additional bytes | Segments | Radix entries | Build ms range, four processes |',
    '| --- | ---: | ---: | ---: | ---: | ---: |']
for family in ['v4','v6','asn']:
    for error in main['epsilon_values']:
        rows=[cache(r) for r in large if r['scenario']==family and r['epsilon']==error]
        sizes={sum(r['allocation_bytes']) for r in rows};assert len(sizes)==1
        dims={tuple(tuple(x) for x in r['dimensions']) for r in rows};assert len(dims)==1
        shape=dims.pop();times=[r['build_ns']/1e6 for r in rows]
        lines.append(f'| {family} | {error} | {sizes.pop():,} | {sum(d[1] for d in shape)} | {sum(d[2] for d in shape)} | {min(times):.3f}-{max(times):.3f} |')
lines+=['','## Untimed guard diagnostics','']
for collection,label in [(main,'Primary'),(fresh,'Repeat')]:
    probes=[r for run in collection['runs'] for r in run['rows'] if r['phase']=='probe' and r['method']==702]
    eligible=sum(r['stats'][0] for r in probes);fallback=sum(r['stats'][1] for r in probes)
    lines.append(f'{label}: {fallback:,} full-search recoveries / {eligible:,} eligible model queries; binary/radix probe statistics agree.')
lines+=['','These include model domain fast paths. Probe counts are outside timing and',
    'do not measure branch/cache hardware events. Forced bad-prediction and NaN',
    'tests qualify recovery even if ordinary corpora need none. A fallback-heavy',
    'throughput/latency profile is not measured by these diagnostics.','',
    '## Linked assembly and counters','',
    'Retained complete estimate/search symbol bodies cover both native key widths.',
    'Generic execute summaries include inlined dispatch, emission and fallbacks.',
    'Static code observations identify floating interpolation, conversions, bounds',
    'and branch/search work; they do not establish runtime causality or cost.',
    'Counter probe: '+asm['counter_probe']['status']+'.','',
    '| Build | Kernel | Bytes | Floating multiply | Floating divide | Conditional jumps |',
    '| --- | --- | ---: | ---: | ---: | ---: |']
for row in asm['rows']:
    for k in row['kernels']:
        c=k['instruction_counts'];kind=k['symbol'].split('::')[-1]
        lines.append(f"| {row['profile']} | {kind} | {k['symbol_bytes']} | {c['float_multiply']} | {c['float_divide']} | {c['conditional_jumps']} |")
lines+=['','## 20M primary medians','']+table([r for r in main['runs'] if r['records']==20_000_000])
lines+=['','## Fresh-process repeat medians','']+table(fresh['runs'])
lines+=['','## 1M primary medians','']+table([r for r in main['runs'] if r['records']==1_000_000])
lines+=['','## Original shape medians','']+table([r for r in main['runs'] if r['records']==65_536 and not r['scenario'].startswith(('curved-','clustered-'))])
lines+=['','## Nonlinear shape medians','']+table([r for r in main['runs'] if r['scenario'].startswith(('curved-','clustered-'))])
lines+=['','## Follow-through','',
    'Keep the guarded model as a research candidate. Before production adoption,',
    'measure large nonlinear/native-provider inputs, startup amortization, zero-copy',
    'correction, cold/>RAM/RSS behavior, fallback-heavy traffic and other hosts.',
    'Do not infer a timing-trained error setting or automatic selector. No full',
    'RadixSpline/PGM competitor comparison or universal learned-index win follows.',
    'Next in priority order: P1-08 AVX-512/NEON qualification on actual hardware;',
    'compile-only architecture evidence cannot establish performance. P2 counters/',
    'PGO/build qualification remains a separate task.']
output.write_text('\n'.join(lines)+'\n')
print(output)
