"""Render descriptive measurements: PRIMARY REPEAT ASM OUTPUT."""
import json
import pathlib
import statistics
import sys

primary, repeat, assembly, output = map(pathlib.Path, sys.argv[1:])
main, fresh, asm = [json.loads(p.read_text()) for p in [primary, repeat, assembly]]


def median(run, traffic, variant):
    return statistics.median(row['ns_per_query'] for row in run['rows']
                             if row['phase'] == 'measure'
                             and row['traffic'] == traffic and row['lanes'] == variant)


def cache(run):
    return next(row for row in run['rows'] if row['phase'] == 'cache')


def table(runs):
    lines = ['| Family / shape | Build | Traffic | Ordinary | Prior SIMD (ID) | Sorted | Eyt (ID) | B+ binary | B+ count | B+ AVX2 count | B+ ordered mask |',
             '| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |']
    for run in runs:
        for traffic in sorted({r['traffic'] for r in run['rows'] if 'traffic' in r}):
            prior = min([108, 112], key=lambda v: median(run, traffic, v))
            eyt = min([301, 302], key=lambda v: median(run, traffic, v))
            values = [f'{median(run, traffic, 0):.1f}',
                      f'{median(run, traffic, prior):.1f} ({prior})',
                      f'{median(run, traffic, 300):.1f}',
                      f'{median(run, traffic, eyt):.1f} ({eyt})']
            values += [f'{median(run, traffic, v):.1f}' for v in [400, 401, 402, 404]]
            lines.append('| ' + ' | '.join([run['scenario'], run['profile'], traffic] + values) + ' |')
    return lines


lines = ['# Flat B+ node-search results - 2026-11-04', '',
         'Decision: retain the qualified prototype as research. No automatic RAM',
         'cache, public API, encoded layout or default-search change is adopted.',
         'The measurements below qualify this implementation and workload, rather',
         'than establish a universal layout winner.', '',
         '[Methodology/reproduction](README.md),',
         '[primary matrix](../results/2026-11-04-flat-btree.json),',
         '[fresh-process repeats](../results/2026-11-04-flat-btree-repeat.json),',
         '[assembly/counter probe](../results/2026-11-04-flat-btree-asm.json),',
         '[integrity verification](../results/2026-11-04-flat-btree-verification.json).', '',
         '## Measurement boundary', '',
         '32 primary processes and six fresh-process repeats produce 2,240 timings.',
         'All original assertions and complete independent original-input query',
         'oracles pass, including ordered targets/IDs, duplicates, borrowed payload',
         'bytes/pointers, mixed families, empty/partial batches, broad-route',
         'fallbacks and typed callback-error prefixes. Forced scalar controls also',
         'pass. Fifteen generated-core tests include independent B+ lower/upper',
         'bounds, incomplete levels through 4097 keys, unsigned half boundaries',
         'and alignment/partial-node SIMD parity. Prepared sources reproduce in a',
         'shadow checkout; header-derived capacities and receipt totals agree.', '',
         'Four-trial median ns/query is aggregate warm throughput cost, not latency',
         'percentiles. Forward/reverse ordering and one warmup corpus precede each',
         'timing. Prior SIMD and Eyt columns report the minimum median of two',
         'methods after measurement, with the winning ID; these are comparison',
         'references, not dispatch policies. All trials remain in receipts.',
         'IP112 duplicates IP108; default/SIMD builds change existing packed tails,',
         'while explicit cross-query and node AVX2 controls exist in both builds.', '',
         'Every mirror searches global fences with identical key widths. Original',
         'IPv4 methods retain the directory, so comparisons with ordinary/prior',
         'also change routing scope, RAM copies and encoded-key density. B+ leaves',
         'preserve sorted positions without per-key rank maps. Packed tails,',
         'target decoding, payload access and interval fallbacks are unchanged.',
         'All comparator caches coexist; construction is outside lookup timing.',
         'PlanB motivates the layout; this is not its LPM transformation, batching,',
         'prefetch or AVX-512 implementation. See the primary reference in README.', '',
         '## Primary 20M cases', '']
lines += table([r for r in main['runs'] if r['records'] == 20_000_000])
lines += ['', '## Fresh-process 20M repeats', '',
          'Same frozen executable/input/corpus, rebuilt caches in a new process.',
          'Four trials cannot isolate host load, frequency or cache state, or',
          'establish statistical significance. Compare both runs before adopting.', '']
lines += table(fresh['runs'])
lines += ['', 'Fresh default-build comparisons (ns/query):', '']
for family in ['v4', 'v6', 'asn']:
    run = next(r for r in fresh['runs'] if r['scenario'] == family and r['profile'] == 'default')
    for traffic in ['uniform', 'hot']:
        best = min([400, 401, 402, 404], key=lambda v: median(run, traffic, v))
        lines.append(f'- {family} {traffic}: B+ {best} {median(run, traffic, best):.1f}; '
                     f'ordinary {median(run, traffic, 0):.1f}; sorted {median(run, traffic, 300):.1f}; '
                     f'cross-query108 {median(run, traffic, 108):.1f}.')
lines += ['', '## Construction and memory', '',
          'Capacity bytes include leaf/internal-key arrays and level-range elements;',
          'they exclude Vec/Cache objects, allocator overhead, mapping/validation',
          'memory and RSS. B+ leaves are separate from the sorted control. All',
          'layouts coexist; construction time includes all layouts, not an isolated',
          'B+ build or original mapping/validation. The 512 MiB combined logical',
          'allocation budget is checked before construction.', '',
          '| Family | Build | B+ leaves MiB | Internal keys MiB | Level bytes | B+ total MiB | All caches MiB | Primary build ms | Fresh build ms |',
          '| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |']
for run in [r for r in main['runs'] if r['records'] == 20_000_000]:
    a = cache(run)['allocation_bytes']
    b = a[9:]
    other = next(r for r in fresh['runs'] if r['scenario'] == run['scenario'] and r['profile'] == run['profile'])
    lines.append('| ' + ' | '.join([run['scenario'], run['profile'],
                 f'{sum(b[::3])/2**20:.3f}', f'{sum(b[1::3])/2**20:.3f}', str(sum(b[2::3])),
                 f'{sum(b)/2**20:.3f}', f'{sum(a)/2**20:.3f}',
                 f"{cache(run)['build_ns']/1e6:.3f}", f"{cache(other)['build_ns']/1e6:.3f}"]) + ' |')
lines += ['', 'No encoded file bytes change. Candidate-only deployment might discard',
          'the comparison caches, but its startup/RSS and API/resource policy need',
          'separate measurements. Natural Vec alignment and unpadded internal-level',
          'starts do not guarantee a node fits within one physical cache line.', '',
          '## Primary 1M cases', '']
lines += table([r for r in main['runs'] if r['records'] == 1_000_000])
lines += ['', '## Shape controls (65,536 assertions)', '',
          'The 256-query warm corpus can favor predictable branches/cache residency.',
          'Deep-prefix/duplicate-IP interval cases measure existing fallback overhead,',
          'not B+ interval traversal. Shared/unique payloads remain separate.', '']
lines += table([r for r in main['runs'] if r['records'] == 65_536])
lines += ['', '## Linked code and limitations', '',
          'Complete B+ bound specializations and explicit AVX2 helpers are retained',
          'in the assembly receipt. Static whole-function instruction/branch counts',
          'include dispatch/fallback/bounds-failure paths; they are not runtime',
          'counts or causal explanations of measured changes. Count401 allows',
          'compiler auto-vectorization; it is not a forced scalar instruction test.', '',
          'The exploratory implementation motivated an additional ordered-mask',
          'variant: portable AVX2 count32 emitted software population-count',
          'arithmetic. Final receipts rerun the entire matrix/repeat set with404',
          'included; exploratory timings are not merged. Ordered404 exploits the',
          'sorted-prefix invariant and can stop after the first partial vector.',
          'It changes both mask reduction and early-exit behavior, so differences',
          'from402 do not isolate a single instruction or prove causality.', '',
          'Linked ordered kernels use vector compares/movemasks and TZCNT-encoded',
          'prefix reduction. The count32 kernel retains software population-count',
          'arithmetic (multiplications/shifts) in this portable build. An outlined',
          'node-dispatch closure and four ISA-detection wrappers remain in linked',
          'code; their complete bodies are retained too. Specializing/inlining the',
          'dispatch and hoisting feature detection are concrete P1-05 ablations.',
          'Instruction presence identifies mechanisms, not their runtime cost.', '',
          '| Build | Symbol | Bytes | Conditional moves | Set condition | Conditional jumps |',
          '| --- | --- | ---: | ---: | ---: | ---: |']
for row in asm['rows']:
    for k in row['kernels']:
        c = k['instruction_counts']
        lines.append(f"| {row['profile']} | `{k['symbol'].split(' ', 3)[-1]}` | {k['symbol_bytes']} | "
                     f"{c['conditional_moves']} | {c['set_condition']} | {c['conditional_jumps']} |")
lines += ['', f"Hardware-counter probe: **{asm['counter_probe']['status']}**. No workload counters were collected.", '',
          'This x86-64/WSL experiment has synthetic fixed warm corpora and uncontrolled',
          'host effects. It does not establish cold/>RAM/native-provider capacity,',
          'MMDB superiority, tail latency, significance or other-architecture performance.',
          'No prefetch, AVX-512/NEON, new cross-query batching, cache-line alignment',
          'guarantee or candidate-only RSS/startup result is introduced.', '',
          'Next: P1-05 traversal/prefetch/alignment ablations; P1-06 owns optional',
          'memory-budgeted selection and directory-preserving variants. Keep this',
          'node-search evidence as a candidate, not an automatic deployment policy.', '']
output.write_text('\n'.join(lines), newline='\n')
print(output)
