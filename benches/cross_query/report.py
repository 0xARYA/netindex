"""Render descriptive results: PRIMARY REPEAT ASM OUTPUT."""
import json
import pathlib
import statistics
import sys

primary, repeat, assembly, output = map(pathlib.Path, sys.argv[1:])
main, fresh, asm = [json.loads(p.read_text()) for p in [primary, repeat, assembly]]


def median(run, traffic, variant):
    return statistics.median(r['ns_per_query'] for r in run['rows']
                             if r['phase'] == 'measure' and r['traffic'] == traffic and r['lanes'] == variant)


def table(runs):
    rows = ['| Family / shape | Build | Traffic | Scalar | Best interleaved (lanes) | AVX2 4 | AVX2 8 | ASN u32 8 / IP duplicate |',
            '| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |']
    for run in runs:
        for traffic in sorted({r['traffic'] for r in run['rows']}):
            best = min([4, 8, 16], key=lambda n: median(run, traffic, n))
            values = [f'{median(run,traffic,0):.1f}', f'{median(run,traffic,best):.1f} ({best})']
            values += [f'{median(run,traffic,n):.1f}' for n in [104, 108, 112]]
            rows.append('| '+' | '.join([run['scenario'],run['profile'],traffic]+values)+' |')
    return rows


lines = ['# Cross-query SIMD results - 2026-11-04', '',
    'Decision: retain these kernels as research; adopt no library default or public',
    'batch API. Large uniform cases can benefit substantially, while hot/small',
    'cases regress. No single kernel establishes dominance across the matrix.', '',
    '[Reproduction](README.md), [primary receipt](../results/2026-11-04-cross-query.json),',
    '[fresh repeats](../results/2026-11-04-cross-query-repeat.json),',
    '[assembly/counter probe](../results/2026-11-04-cross-query-asm.json),',
    '[integrity verification](../results/2026-11-04-cross-query-verification.json).', '',
    '## Protocol and correctness', '',
    '32 primary processes plus six fresh repeats produced 1,792 timing measurements.',
    'All original assertions and complete oracle/scalar/candidate output sequences',
    'agree, including IDs, targets, duplicates, payload bytes and borrowed pointers.',
    'Mixed-family interval/packed/broad-route fixtures, empty/partial batches and',
    'typed callback-error prefixes pass. Forced scalar versions of every new kernel',
    'are checked on every corpus and error fixture. Twelve generated-core unit tests',
    'pass, including linear search oracles at 32 byte alignments and unsigned',
    'boundaries, inactive groups, row-count boundaries and invalid bounds.', '',
    'All methods share each executable, validated immutable input and query corpus.',
    'Builds differ only in optional existing within-query tail SIMD; cross-query',
    'AVX2 is active in both. Four forward/reverse trials include setup, ISA/bounds',
    'checks, query-major visitors and payload consumption. No timed output allocation,',
    'application decoding or JSON. Portable release/codegen-units=1, no PGO/LTO.', '',
    'Numbers below are four-trial medians of aggregate ns/query, not latency',
    'percentiles. Best interleaved is the minimum median across P1-01 groups 4/8/16',
    'for that row: a post-hoc comparison reference, not an adaptive routing policy.',
    'One-lane controls and every individual trial remain in the receipt. IP variant',
    '112 calls the same kernel as 108 and is a duplicate timing control; only ASN',
    '112 uses native eight-lane u32 gathers.', '',
    '## Primary 20M cases', '']
lines += table([r for r in main['runs'] if r['records'] == 20_000_000])
lines += ['', '## Fresh-process 20M repeats', '',
          'All six family/build cases repeat without changing source, executable,',
          'input, queries or protocol. Absolute timings and relative rankings vary;',
          'do not treat a single best number as a stable statistical speedup.', '']
lines += table(fresh['runs'])
lines += ['', 'For the fresh default-build uniform cases:', '']
for family, variant in [('v4',108),('v6',108),('asn',112)]:
    run = next(r for r in fresh['runs'] if r['scenario']==family and r['profile']=='default')
    base = median(run,'uniform',0)
    candidate = median(run,'uniform',variant)
    best = min(median(run,'uniform',n) for n in [4,8,16])
    hot = median(run,'hot',variant)/median(run,'hot',0)
    lines.append(f'- {family}: variant {variant}, {candidate:.1f} versus scalar {base:.1f} ns/query '
                 f'({100*(1-candidate/base):.1f}% lower); best interleaved {best:.1f}. '
                 f'Hot cost is {hot:.2f}x scalar.')
v4fresh = next(r for r in fresh['runs'] if r['scenario']=='v4' and r['profile']=='default')
lines += ['', f"The default IPv4 duplicate controls 108/112 have fresh medians {median(v4fresh,'uniform',108):.1f}",
    f"and {median(v4fresh,'uniform',112):.1f} ns/query despite invoking the same IP algorithm.",
    'This exposes timing-position/code-layout/host uncertainty; the data cannot',
    'attribute the difference to SIMD work. Native ASN versus 64-bit eight-query',
    'kernel rankings also change between primary and fresh cases. Its lower hot',
    'cost does not establish universal superiority. No significance claim is made.']
lines += ['', '## Shape controls (65,536 assertions)', '']
lines += table([r for r in main['runs'] if r['records']==65_536])
lines += ['', 'Dense/sparse ASN and sparse IPv6 expose small warm-index costs with shared',
    'and unique payloads. Deep-prefix/duplicate-address cases retain scalar interval',
    'traversal: they check fallback semantics/overhead, not vector interval traversal.', '',
    '## Assembly and resource boundary', '',
    'Each executable contains five AVX2 kernel specializations. The 64-bit kernels',
    'use VPGATHERQQ and VPCMPGTQ; native ASN eight-lane uses VPGATHERDD and VPCMPGTD.',
    'Unsigned keys use sign-bit transforms; IP fences compare both 64-bit halves.',
    'The four-query 64-bit kernels have no explicit RSP-addressed memory operands',
    '(register saves are separate). The eight-query 64-bit kernels retain a loop and',
    'stack-backed state; native u32 ASN provides a separate compact-state comparison.',
    'Static sizes/counts and excerpts are in the assembly receipt. They describe',
    'whole functions, not runtime instructions, spill costs or cache misses.', '',
    'The perf probe failed because this WSL kernel lacks its matching perf tool.',
    'No hardware-counter or achieved memory-parallelism claim is made. Guest CPU',
    'pinning cannot control host scheduling/cache/frequency conditions. Comparing',
    'different builds or the old P1-01 executable does not isolate a causal effect.', '',
    'The first exploratory pass used 4/8-query 64-bit kernels. Inspection motivated',
    'adding the native u32 ASN candidate. All final methods and controls were then',
    'remeasured in full; final receipts refer to the frozen final sources only.', '',
    '## Decision and next work', '',
    '- P1-02 is complete as a qualified prototype comparison. Keep the useful large-',
    '  uniform candidates and controls; hot/small regressions prevent blanket adoption.',
    '- Preserve existing tail SIMD, fallbacks, on-disk bytes and public APIs. There is',
    '  no package version bump or registry release for this research checkpoint.',
    '- Next priority is P1-03 Eytzinger fence layouts. Reuse identical semantic',
    '  contracts and scalar controls; keep SIMD/interleaving results as comparators.',
    '- P1-06 can later evaluate lower-overhead family-specific entry points, dispatch',
    '  hoisting, explicit register-resident 64-bit state and workload-aware selection.',
    '  None is assumed to win without measurement.',
    '- ARM/AVX-512, cold/>RAM operation, application traffic, hardware counters and',
    '  held-out PGO/build qualification remain separate tasks. Four-trial warm',
    '  synthetic evidence establishes no universal advantage or production capacity.']
output.write_text('\n'.join(lines)+'\n', encoding='utf-8')
print(output)
