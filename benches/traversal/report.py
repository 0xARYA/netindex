"""Render retained traversal evidence: PRIMARY REPEAT ASM OUTPUT."""
import json
import pathlib
import statistics
import sys

primary, repeat, assembly, output = map(pathlib.Path, sys.argv[1:])
main, fresh, asm = [json.loads(p.read_text()) for p in [primary, repeat, assembly]]


def median(run, traffic, variant):
    return statistics.median(row['ns_per_query'] for row in run['rows']
                             if row['phase'] == 'measure' and row['traffic'] == traffic
                             and row['lanes'] == variant)


def cache(run):
    return next(r for r in run['rows'] if r['phase'] == 'cache')


def table(runs, variants, labels):
    lines = ['| Family / shape | Build | Traffic | ' + ' | '.join(labels) + ' |',
             '| --- | --- | --- | ' + ' | '.join(['---:'] * len(variants)) + ' |']
    for run in runs:
        for traffic in sorted({r['traffic'] for r in run['rows'] if 'traffic' in r}):
            values = [f'{median(run, traffic, v):.1f}' for v in variants]
            lines.append('| ' + ' | '.join([run['scenario'], run['profile'], traffic] + values) + ' |')
    return lines


lines = ['# Traversal, prefetch and alignment results - 2026-11-04', '',
         'Decision: retain the measured candidates as research for memory-budgeted',
         'selection. No automatic cache, production default, public API or format',
         'change is adopted. These results qualify the tested implementations and',
         'workloads; they do not establish universal dominance.', '',
         '[Reproduction](README.md), [primary matrix](../results/2026-11-04-traversal.json),',
         '[fresh-process repeats](../results/2026-11-04-traversal-repeat.json),',
         '[assembly/counter probe](../results/2026-11-04-traversal-asm.json),',
         '[integrity verification](../results/2026-11-04-traversal-verification.json).', '',
         '## Scope and correctness', '',
         '32 primary processes and six fresh-process repeats yield 3,808 timings.',
         'All original assertions and complete independent original-input oracles',
         'pass, including ordered targets/IDs, duplicates, borrowed payload bytes/',
         'pointers, partial/empty/mixed batches, broad-route fallback and typed',
         'callback-error prefixes. Forced scalar paths and 17 generated-core tests',
         'pass. Independent linear bounds cover all eight Eytzinger combinations,',
         'specialized B+ traversal, incomplete trees through 4097, duplicates and',
         'unsigned boundaries. Preparation reproduces in a shadow checkout; hashes,',
         'raw logs, header-derived capacities, alignment and timing coordinates agree.', '',
         'Four-trial median ns/query is aggregate warm throughput cost, not query/',
         'batch latency percentiles. One warmup corpus precedes every timing, with',
         'forward/reverse trial ordering. All comparators use the same executable,',
         'input and resident caches within each build. Default/SIMD builds change',
         'existing packed tails; explicit AVX2 methods exist in both builds.', '',
         'New Eytzinger methods share identical 64-byte group accessors. Factors:',
         '**N/64** natural/guaranteed 64-byte alignment; **C/A** candidate/arithmetic child',
         'selection; **P** guarded T0 prefetch. Columns 500..507 cover the complete',
         '2x2x2 design. Natural base alignment is recorded, not assumed.',
         'Prefetch targets one descendant group four ASN/two IP levels ahead,',
         'clipped before pointer formation. Only group-start line is requested;',
         'natural groups can straddle lines. No distance/hint sweep is measured.', '',
         'All mirrors search global fences. Original IPv4 ordinary/prior methods',
         'retain their directory, changing routing scope and key density in those',
         'comparisons. Existing packed tails, decoding and borrowed emission follow.',
         'Interval/broad-route readers retain their ordinary visitor; their shape',
         'measurements are fallback controls rather than prefetched interval trees.', '',
         '## Primary 20M Eytzinger factorial', '']
eyt_variants = [0] + list(range(500, 508))
eyt_labels = ['Ordinary', 'N C500', 'N A501', 'N C+P502', 'N A+P503',
              '64 C504', '64 A505', '64 C+P506', '64 A+P507']
large = [r for r in main['runs'] if r['records'] == 20_000_000]
lines += table(large, eyt_variants, eyt_labels)
lines += ['', '## Fresh-process 20M Eytzinger factorial', '',
          'Same frozen executable/input/corpus with rebuilt caches in new processes.',
          'Trial medians cannot isolate WSL host load/frequency/cache drift or prove',
          'statistical significance. Compare both primary and repeat evidence.', '']
lines += table(fresh['runs'], eyt_variants, eyt_labels)
lines += ['', 'Fresh default-build paired arithmetic comparisons (ns/query):', '']
for family in ['v4', 'v6', 'asn']:
    r = next(r for r in fresh['runs'] if r['scenario'] == family and r['profile'] == 'default')
    for t in ['uniform', 'hot']:
        lines.append(f'- {family} {t}: natural/noP501 {median(r,t,501):.1f}, '
                     f'aligned/noP505 {median(r,t,505):.1f}, aligned/P507 {median(r,t,507):.1f}; '
                     f'ordinary {median(r,t,0):.1f}.')
lines += ['', '## B+ dispatch/specialization intervention', '',
          '401/404 retain legacy count/ordered visitors. 520 specializes count;',
          '521 specializes the AVX2 walk, makes direct helpers inline-capable and',
          'detects AVX2 outside node loops. Unsupported/disabled cases use count.',
          'These are combined implementation interventions, not independent causal',
          'estimates of inlining, dispatch and feature detection. Arrays are shared.', '',
          'Primary 20M:', '']
b_variants = [0, 108, 300, 301, 302, 401, 404, 520, 521]
b_labels = ['Ordinary', 'Prior108', 'Sorted300', 'Eyt301', 'Eyt302',
            'Old count401', 'Old ordered404', 'Count520', 'Ordered521']
lines += table(large, b_variants, b_labels)
lines += ['', 'Fresh 20M repeats:', '']
lines += table(fresh['runs'], b_variants, b_labels)
lines += ['', '## Capacity, construction and physical alignment', '',
          'All arrays coexist even for ordinary timings. Capacities include sentinel/',
          'group padding and level-range elements, exclude allocator/object overhead,',
          'mapping/validation memory and RSS. Build time covers all layouts together,',
          'not candidate-only startup; the requested logical budget is 512 MiB.', '',
          '| Family | Build | New mirrors MiB | All caches MiB | Natural/64 base mod 64 | Primary build ms | Fresh build ms |',
          '| --- | --- | ---: | ---: | --- | ---: | ---: |']
for r in large:
    c = cache(r)
    other = next(s for s in fresh['runs'] if s['scenario'] == r['scenario'] and s['profile'] == r['profile'])
    index = {'asn': 0, 'v4': 2, 'v6': 4}[r['scenario']]
    mods = c['alignment_mod64'][index:index+2]
    lines.append('| ' + ' | '.join([r['scenario'], r['profile'],
        f"{sum(c['allocation_bytes'][18:])/2**20:.3f}", f"{sum(c['allocation_bytes'])/2**20:.3f}",
        str(mods), f"{c['build_ns']/1e6:.3f}", f"{cache(other)['build_ns']/1e6:.3f}"]) + ' |')
lines += ['', 'Per-process actual alignments and each array capacity remain in receipts.',
          'The grouped mirror vs legacy Eytzinger comparison changes accessors and',
          'specialization as well as storage. Use the factorial columns for paired',
          'alignment/prefetch/traversal comparisons. Candidate-only memory/startup',
          'would differ if comparison caches were removed; it is not measured here.', '',
          '## 1M and shape controls', '',
          'Below, Eyt best is the minimum median over 500..507 after measurement,',
          'with its ID. This is a comparison summary, not a measured dispatch policy.',
          'Full per-method trials remain in the primary receipt. Shapes reuse a',
          '256-query corpus and can favor predictable branches/cache residency.', '',
          '| Records | Family / shape | Build | Traffic | Ordinary | Eyt best (ID) | Count520 | Ordered521 |',
          '| ---: | --- | --- | --- | ---: | ---: | ---: | ---: |']
for r in [r for r in main['runs'] if r['records'] != 20_000_000]:
    for t in sorted({row['traffic'] for row in r['rows'] if 'traffic' in row}):
        best = min(range(500,508), key=lambda v:median(r,t,v))
        lines.append('| ' + ' | '.join([str(r['records']), r['scenario'], r['profile'], t,
            f'{median(r,t,0):.1f}', f'{median(r,t,best):.1f} ({best})',
            f'{median(r,t,520):.1f}', f'{median(r,t,521):.1f}']) + ' |')
lines += ['', '## Linked assembly and limitations', '',
          'Full bounded Eytzinger factor/two-width walks and specialized B+ walks,',
          'legacy dispatch and wrappers are retained. Static instruction/branch/',
          'prefetch counts include all helper paths, not runtime execution counts.',
          'Arithmetic source is not a claim of a fully branch-free algorithm.', '',
          'All four specialized B+ walks per executable contain no calls before',
          'their normal return; the three remaining call sites in each body lie',
          'in bounds-failure blocks after that return. Both AVX2 key-width walks',
          'contain direct vector comparisons: node helpers are inlined. ISA detection',
          'is outside the node loop in source. These code observations establish',
          'mechanisms, not individual runtime savings or a causal speedup split.', '',
          '| Build | Eyt walks | Walks with T0 | T0 sites | B+ count/vector walks |',
          '| --- | ---: | ---: | ---: | ---: |']
for r in asm['rows']:
    e = [k for k in r['kernels'] if 'study_eytzinger::Tree<K>::walk' in k['symbol']]
    b = [k for k in r['kernels'] if '::walk_count' in k['symbol'] or '::walk_vector' in k['symbol']]
    lines.append(f"| {r['profile']} | {len(e)} | {sum(k['instruction_counts']['prefetch_t0']>0 for k in e)} | "
                 f"{sum(k['instruction_counts']['prefetch_t0'] for k in e)} | {len(b)} |")
lines += ['', f"Hardware-counter probe: **{asm['counter_probe']['status']}**. No workload counters collected.", '',
          'One x86-64 WSL host and synthetic warm corpora cannot establish cold/>RAM,',
          'native-provider, latency-percentile, significance or cross-architecture',
          'performance. No PGO/LTO, AVX-512/NEON, new batch kernel or on-disk',
          'reordering is introduced. ISA/prefetch instructions identify mechanisms;',
          'without counters they do not establish why a timing changed.', '',
          'Next: P1-06 optional memory-budgeted strategy selection, including native',
          'IPv4 key widths and directory-aware forests. Keep default/adaptive policy',
          'decisions separate from post-hoc fastest-variant summaries.', '']
output.write_text('\n'.join(lines), newline='\n')
print(output)
