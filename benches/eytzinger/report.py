"""Render descriptive measurements: PRIMARY REPEAT ASM OUTPUT."""
import json
import pathlib
import statistics
import sys

primary, repeat, assembly, output = map(pathlib.Path, sys.argv[1:])
main, fresh, asm = [json.loads(p.read_text()) for p in [primary, repeat, assembly]]


def median(run, traffic, variant):
    return statistics.median(r['ns_per_query'] for r in run['rows']
                             if r['phase']=='measure' and r['traffic']==traffic and r['lanes']==variant)


def cache(run):
    return next(row for row in run['rows'] if row['phase']=='cache')


def table(runs):
    rows=['| Family / shape | Build | Traffic | Ordinary | Best prior (ID) | Sorted mirror | Eyt candidate | Eyt path |',
          '| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |']
    for run in runs:
        for traffic in sorted({r['traffic'] for r in run['rows'] if 'traffic' in r}):
            best=min([8,16,108,112],key=lambda n:median(run,traffic,n))
            values=[f'{median(run,traffic,0):.1f}',f'{median(run,traffic,best):.1f} ({best})']
            values += [f'{median(run,traffic,n):.1f}' for n in [300,301,302]]
            rows.append('| '+' | '.join([run['scenario'],run['profile'],traffic]+values)+' |')
    return rows


lines=['# Eytzinger RAM-layout results - 2026-11-04','',
    'Decision: retain the measured research layouts; do not adopt a library default',
    'or format change. Candidate tracking helps these ASN controls, but global IP',
    'heaps and arithmetic traversal regress elsewhere, with added RAM/startup costs.','',
    '[Reproduction and methodology](README.md), [primary matrix](../results/2026-11-04-eytzinger.json),',
    '[fresh repeats](../results/2026-11-04-eytzinger-repeat.json),',
    '[linked-code/counter probe](../results/2026-11-04-eytzinger-asm.json),',
    '[integrity verification](../results/2026-11-04-eytzinger-verification.json).','',
    '## Measurement boundary','',
    'An earlier matrix was interrupted by a filesystem I/O failure and WSL shutdown.',
    'Its timings were not accepted. After storage/VM recovery, the entire primary',
    'matrix and all fresh-process cases were rerun; receipts below are from that',
    'post-recovery run only. No partial pre-recovery timing is merged into them.','',
    '32 primary processes plus six fresh repeats produced 1,792 timing measurements.',
    'All original assertions, complete original-target oracle matches, scalar output,',
    'borrowed pointers, duplicates, mixed families, empty/partial batches, broad-route',
    'fallbacks and typed callback-error prefixes pass. Thirteen generated-core tests',
    'include linear lower/upper-bound oracles for incomplete trees and unsigned',
    'u32/u128 extremes. Final prepared sources reproduce in a shadow checkout.','',
    'Four-trial median aggregate ns/query follows; these are throughput costs, not',
    'query/batch latency percentiles. One warmup corpus precedes each timing.',
    'Best prior is the minimum median across interleaving 8/16 and cross-query SIMD',
    '108/112 for that row. It is a post-hoc comparison reference, not a dispatch policy.',
    'All individual trials and methods remain in receipts. IP112 duplicates IP108.','',
    'Ordinary/prior IPv4 paths keep their directory. Sorted mirror and both Eytzinger',
    'methods search all packed fences, using identical key widths. Comparing with',
    'the sorted mirror assesses the layout/rank-mapping tradeoff; comparing with',
    'ordinary also changes key density, RAM copying and IPv4 routing scope.',
    'Packed tail search, decoding/payload access and interval fallbacks are unchanged.',
    'All mirrors stay resident during every comparator. Lookups exclude cache build.',
    'No on-disk layout or production library code changes.','',
    '## Primary 20M cases','']
lines += table([r for r in main['runs'] if r['records']==20_000_000])
lines += ['', '## Fresh-process 20M repeats','',
    'Every family/build repeats in a fresh process with the same executable, input',
    'and corpus. Startup rebuilds all RAM mirrors. Four trials per method cannot',
    'control host load/frequency/cache state or establish statistical significance.','']
lines += table(fresh['runs'])
lines += ['', 'Fresh default-build comparisons (ns/query):','']
for family in ['v4','v6','asn']:
    r=next(r for r in fresh['runs'] if r['scenario']==family and r['profile']=='default')
    for t in ['uniform','hot']:
        best=min([301,302],key=lambda n:median(r,t,n))
        lines.append(f'- {family} {t}: Eyt {best} {median(r,t,best):.1f}; sorted mirror {median(r,t,300):.1f}; ordinary {median(r,t,0):.1f}.')
lines += ['', '## Cache construction and memory','',
    'Capacity bytes cover array elements only: no allocator overhead, cache metadata,',
    'mapping, validation memory or process RSS. All mirrors coexist in these runs.',
    'Build time includes sorted extraction, heap permutation and rank construction;',
    'it does not isolate candidate-only startup or include original reader validation.',
    'The 512 MiB budget bounds requested logical array sizes before allocation.','',
    '| Family | Build | Sorted MiB | Eyt keys/ranks MiB | Combined MiB | Primary build ms | Fresh build ms |',
    '| --- | --- | ---: | ---: | ---: | ---: | ---: |']
for run in [r for r in main['runs'] if r['records']==20_000_000]:
    c=cache(run); a=c['allocation_bytes']; other=next(r for r in fresh['runs'] if r['scenario']==run['scenario'] and r['profile']==run['profile'])
    sorted_bytes=sum(a[::3]);total=sum(a)
    lines.append(f"| {run['scenario']} | {run['profile']} | {sorted_bytes/2**20:.3f} | {(total-sorted_bytes)/2**20:.3f} | {total/2**20:.3f} | {c['build_ns']/1e6:.3f} | {cache(other)['build_ns']/1e6:.3f} |")
lines += ['', 'Original file bytes are unchanged. Original-rank maps add to the heap cost',
    'and restore insertion IDs/duplicate sequences and original block',
    'positions. A computed rank mapping is not evaluated here. Shared/unique payload',
    'files retain separate checks and measurements.','',
    '## Shape controls (65,536 assertions)','']
lines += table([r for r in main['runs'] if r['records']==65_536])
lines += ['', 'Deep-prefix/duplicate-address cases retain ordinary interval traversal.',
    'They measure fallback overhead/correctness rather than Eytzinger interval trees.',
    'Dense/sparse ASN and 16-byte-delta IPv6 controls expose small warm-index costs.','',
    '## Assembly and limitations','',
    'Each executable has two key-width specializations of Tree::bound, containing',
    'all three mirror methods behind runtime dispatch. The linked-code receipt keeps',
    'complete bounded helper bodies and conditional jump/move/set counts. Those',
    'counts include all paths: they are not dynamic instructions, cache misses or',
    'causal evidence for a timing difference. Arithmetic source is not a claim of',
    'branch-free generated search. No explicit prefetch or cache-line alignment is',
    'added; the research paper\'s tuned prefetch implementation is not reproduced.','',
    'The matching WSL perf tool is unavailable; no hardware counters were measured.',
    'Portable release/codegen-units=1, optional existing tail SIMD, no PGO/LTO/custom',
    'target flags. These are synthetic warm fixed corpora on one live x86-64 WSL',
    'host. No cold/>RAM, other-architecture, native-provider or production capacity',
    'claim follows. Cross-build or old-executable comparisons do not isolate layout.']
lines += ['', '## Generated-code observations and decision','',
    'The u32 arithmetic loop uses CMP/SETB/LEA for comparison-to-child calculation,',
    'a conditional loop back-edge, and NOT/TZCNT/shift for path recovery, followed',
    'by original-rank lookup and a bounds-failure path. The sorted path includes a',
    'conditional move; candidate tracking retains data-dependent branches. These',
    'observations confirm distinct emitted strategies without assigning a measured',
    'cost to any instruction or claiming the entire helper is branch-free.','',
    '- P1-03 is complete as a RAM-layout comparison. Retain both Eytzinger methods',
    '  and the sorted mirror as research controls; no default/API/format change.',
    '- Fresh default hot ASN: candidate tracking 18.4 versus ordinary 22.5 ns/query.',
    '  Fresh uniform ASN: 198.4 versus sorted mirror 260.8 and ordinary 277.8;',
    '  earlier cross-query SIMD is faster at 109.1. Heap keys/ranks add 152.588 MiB',
    '  for this 20M case; keeping the sorted control totals 228.882 MiB.',
    '- Small ASN shape gains (roughly 6-10 versus 19-25 ns/query) use only 256',
    '  repeatedly reused synthetic queries. They can favor predictable branches;',
    '  do not generalize them to larger/unseen corpora or application capacity.',
    '- IPv6 arithmetic traversal can beat the matched sorted mirror, while prior',
    '  SIMD remains faster. Fresh SIMD IPv4 global arithmetic traversal is 265.1',
    '  versus ordinary 105.4 ns/query. Losing the directory is a separate tradeoff;',
    '  this result does not reject directory-preserving Eytzinger designs.',
    '- P1-05 should test prefetch, branchless traversal and alignment separately.',
    '  P1-06 can evaluate directory-preserving forests, computed rank mappings',
    '  and memory-budgeted selection with independent corpora. No variant here is',
    '  assumed to be the best possible Eytzinger implementation.',
    '- Next priority: P1-04 flat B+ tree / PlanB-inspired node search, preserving',
    '  complete all-match semantics. Counters, other architectures and held-out',
    '  application PGO remain separate qualification tasks. No version bump or',
    '  registry release is needed for this research checkpoint.']
output.write_text('\n'.join(lines)+'\n',encoding='utf-8')
print(output)
