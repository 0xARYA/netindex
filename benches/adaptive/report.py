"""Render adaptive receipts: PRIMARY REPEAT ASM OUTPUT."""
import json
import pathlib
import statistics
import sys

primary, repeat, assembly, output = map(pathlib.Path, sys.argv[1:])
main, fresh, asm = [json.loads(p.read_text()) for p in [primary, repeat, assembly]]
variants = [0, 108, 600, 601, 602, 603]


def median(run, traffic, variant):
    return statistics.median(row['ns_per_query'] for row in run['rows']
                             if row['phase'] == 'measure' and row['traffic'] == traffic
                             and row['lanes'] == variant)


def cache(run):
    return next(row for row in run['rows'] if row['phase'] == 'cache')


def table(runs):
    lines = ['| Family / shape | Build | Budget MiB | Traffic | Policy 602 route | 0 | 108 | 600 | 601 | 602 | 603 |',
             '| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |']
    for run in runs:
        for plan in [r for r in run['rows'] if r['phase'] == 'plan']:
            traffic = plan['traffic']
            values = [f'{median(run, traffic, v):.1f}' for v in variants]
            lines.append('| ' + ' | '.join([run['scenario'], run['profile'],
                str(run['budget_bytes']//(1<<20)), traffic, str(plan['selected'])] + values) + ' |')
    return lines


lines = ['# Adaptive search results - 2026-11-04', '',
    'Decision: reject the tested cache-presence/prefix heuristic for production.',
    'Retain candidate-only budget enforcement and the failed selector as',
    'reproducible research. No production policy, cache default, public API or',
    'format change is adopted. A larger budget does not imply faster queries.', '',
    '[Protocol/reproduction](README.md), [primary matrix](../results/2026-11-04-adaptive.json),',
    '[fresh-process repeats](../results/2026-11-04-adaptive-repeat.json),',
    '[assembly/counter probe](../results/2026-11-04-adaptive-asm.json),',
    '[integrity verification](../results/2026-11-04-adaptive-verification.json).', '',
    '## Contract and measurement scope', '',
    '96 primary processes and 18 fresh 20M repeats yield 6,624 timings. Each process',
    'holds only its admitted candidate caches. All original assertions and the',
    'complete independent original-input query oracle pass, including ordered',
    'targets/IDs, duplicate multiplicity, borrowed payload bytes/pointers, partial/',
    'empty/mixed batches, broad-route fallback and typed callback-error prefixes.',
    'Forced scalar paths and 18 generated-core tests pass. Preparation reproduces',
    'in a shadow checkout; source/input/executable/raw-log hashes, header-derived',
    'aggregate capacity budgets, policy traces and measurement coordinates agree.', '',
    'Every number below is a four-trial median warm aggregate ns/query. One warmup',
    'precedes each timing; forward/reverse trial ordering, default/SIMD build order',
    'and budget order are controlled. Both builds use runtime AVX2 detection.',
    'This is one WSL host with fixed synthetic corpora. These are throughput costs,',
    'not latency percentiles, statistical-significance claims or cold/>RAM results.',
    'No PGO/LTO/custom target flags are used. Different budgets run in different',
    'processes, so ratios across budgets include process drift.', '',
    'IDs: 0 ordinary; 108 prior cross-query AVX2; 600 global admitted caches/ordered',
    'ASN; 601 directory-aware IPv4/count ASN; 602 full-request prefix policy; 603',
    'the same policy in requests of at most 32 queries. Sampling, dispatch and',
    'request splitting/rebasing are timed. Cache construction is outside lookup',
    'timing. Full-request route traces correspond to 602, not every 603 subrequest.', '',
    '## Budget and startup', '',
    'Admission order is native u32 IPv4 fences, u128 IPv6 fences, then ASN flat B+',
    'leaves/internal keys/level ranges. Empty/unaffordable candidates allocate no',
    'arrays. The bound covers actual additional vector element capacities, not',
    'RSS, mapped input, ordinary directory/validation, object or allocator memory.',
    'Fixed admission is not an optimized allocation policy. Startup below includes',
    'only optional-cache construction after ordinary reader validation.', '',
    '| 20M family | Budget MiB | Additional bytes | Build ms range, four processes |',
    '| --- | ---: | ---: | ---: |']
large = [r for r in main['runs']+fresh['runs'] if r['records']==20_000_000]
for family in ['v4', 'v6', 'asn']:
    for budget in main['budgets_bytes']:
        rows = [cache(r) for r in large if r['scenario']==family and r['budget_bytes']==budget]
        sizes = {sum(r['allocation_bytes']) for r in rows}
        assert len(sizes)==1
        times = [r['build_ns']/1e6 for r in rows]
        lines.append(f'| {family} | {budget//(1<<20)} | {sizes.pop():,} | {min(times):.3f}-{max(times):.3f} |')
lines += ['', '## Selector qualification', '',
    'For the 144 measured 20M workload/build/budget/process combinations, compare',
    'each policy with the fastest observed fixed-control median among 0/108/600/601.',
    'The minimum of several measured controls is an optimistic comparator, not',
    'a deployable oracle. Counts and extrema do not establish significance.', '',
    '| Policy | Within 10% of fastest fixed | Slower than ordinary | Worst ratio to fastest fixed | Worst case |',
    '| --- | ---: | ---: | ---: | --- |']
for variant in [602,603]:
    points=[]
    for collection,label in [(main,'primary'),(fresh,'repeat')]:
        for run in collection['runs']:
            if run['records']!=20_000_000:
                continue
            for traffic in ['uniform','hot','late-wide','late-hot']:
                value=median(run,traffic,variant)
                best=min(median(run,traffic,v) for v in variants[:4])
                points.append((value/best,value>median(run,traffic,0),
                    f"{label} {run['scenario']} {run['profile']} b{run['budget_bytes']//(1<<20)} {traffic}"))
    worst=max(points)
    lines.append(f'| {variant} | {sum(p[0]<=1.1 for p in points)}/{len(points)} | {sum(p[1] for p in points)}/{len(points)} | {worst[0]:.2f}x | {worst[2]} |')
lines += ['', 'Late-wide and late-hot deliberately reverse locality after the first 32',
    'queries. Full-request 602 necessarily retains the decision from that prefix.',
    'Request 32 variant 603 resamples every request, but pays more decision/dispatch',
    'overhead; its first request can still be unrepresentative. Span is only a',
    'proxy for locality. No claim that the heuristic is robust to arbitrary order,',
    'distribution shifts, mixed-family suffixes or batch sizes follows.', '',
    'Read same-process fixed controls before attributing a difference to policy.',
    '601 changes both IPv4 directory use and ASN node search; it is not one global',
    'factorial contrast. Native-width IPv4 arrays plus the existing directory are',
    'qualified as a combined candidate; this study does not isolate width alone.', '',
    '## Linked code and counters', '',
    'Extracted symbol bodies and static instruction counts are retained in the',
    'assembly receipt for the policy chooser, request planners and specialized ASN',
    'scalar/AVX2 walks. Generic execute summaries retain scaled-access examples;',
    'IP searches are inlined there alongside other variants, so those examples',
    'do not isolate the IPv4 loop or width-only effects. Bounds, fallback and emission',
    'remain in timed generic readers. Static counts are not runtime cost or branch/cache',
    'causality. Counter probe status: '+asm['counter_probe']['status']+'.', '',
    'Both builds emit the same 64-byte chooser with four conditional moves, no',
    'conditional jumps and no calls. The 733-byte count walk already contains',
    'four SSE comparison sites; the 1064-byte ordered walk contains 14 AVX2',
    'comparison sites. Both walks have no calls before their first return.',
    'Portable/scalar fallback means no explicit AVX2 dispatch; compiler-generated',
    'SSE remains permitted. More explicit SIMD does not establish a faster path.', '',
    '## 20M primary medians', ''] + table([r for r in main['runs'] if r['records']==20_000_000])
lines += ['', '## 20M fresh-process repeat medians', ''] + table(fresh['runs'])
lines += ['', '## 1M primary medians', ''] + table([r for r in main['runs'] if r['records']==1_000_000])
lines += ['', '## Shape medians', ''] + table([r for r in main['runs'] if r['records']==65_536])
lines += ['', '## Follow-through', '',
    'Keep the explicit budget and cheap fallback contract for future experiments.',
    'A production selector needs representative batch-size/distribution-shift',
    'validation, admission/eviction economics, startup amortization, RSS/cold/>RAM',
    'qualification and evidence on more hosts. Broader sampling or per-request',
    'routing must earn its overhead; no timing-trained tuning is adopted here.',
    'Next in priority order: P1-07 learned fences with exact bounded search and',
    'fallback, measured under the same complete output contract.']
output.write_text('\n'.join(lines)+'\n')
print(output)
