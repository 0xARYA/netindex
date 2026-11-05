"""Prepare an isolated P1-07 core and reuse the frozen P1-01 correctness harness."""
import hashlib
import json
import pathlib
import shutil
import subprocess

here = pathlib.Path(__file__).resolve().parent
repo = here.parents[1]
output = repo / 'target/learned'
core = output / 'library'
core.mkdir(parents=True, exist_ok=False)
shutil.copytree(repo / 'src', core / 'src')
shutil.copyfile(repo / 'README.md', core / 'README.md')
manifest = (repo / 'Cargo.toml').read_text().split('[dev-dependencies]')[0]
manifest = manifest.replace('name = "netindex"', 'name = "netindex-learned-core"\npublish = false', 1)
(core / 'Cargo.toml').write_text(manifest + '\n[workspace]\n', newline='\n')
for name in ['reader', 'packed']:
    path = core / f'src/{name}.rs'
    original = path.read_text()
    fragments = [(here.parent / 'interleaved' / f'{name}.inc.rs').read_text(), (here.parent / 'cross_query' / f'{name}.inc.rs').read_text(), (here / f'{name}.inc.rs').read_text()]
    patch = '\n'.join(fragments)
    marker = original.find('#[cfg(test)]')
    path.write_text(original + '\n' + patch if marker < 0 else original[:marker] + patch + '\n' + original[marker:], newline='\n')
shutil.copyfile(here.parent / 'cross_query/study_simd.rs', core / 'src/study_simd.rs')
shutil.copyfile(here / 'study_learned.rs', core / 'src/study_learned.rs')
path = core / 'src/lib.rs'
path.write_text(path.read_text() + '\nmod study_simd;\nmod study_learned;\n', newline='\n')
main = (here.parent / 'interleaved/src/main.rs').read_text()


def replace_once(old, new):
    global main
    assert main.count(old) == 1, old
    main = main.replace(old, new)


replace_once('const VARIANTS: [usize; 5] = [0, 1, 4, 8, 16];', 'const VARIANTS: [usize; 5] = [0, 108, 700, 701, 702];')
# Forced scalar kernels participate in every full/partial/error precheck, not timing.
main = main.replace('for variant in VARIANTS {', 'for variant in VARIANTS.into_iter().chain([204, 208, 212, 703]) {')
replace_once('        1 => batch::<1, _, _>(reader, queries, emit),', '''        700..=703 => match queries {
            Queries::Asns(q) => reader.study_learned_asns(q, lanes-700, emit),
            Queries::Ips(q) => reader.study_learned_ips(q, lanes-700, emit),
        },
        112 | 212 => match queries {
            Queries::Asns(q) => reader.study_vector_asns32(q, lanes < 200, emit),
            Queries::Ips(_) => vector_batch::<8, _, _>(reader, queries, lanes < 200, emit),
        },
        104 | 204 => vector_batch::<4, _, _>(reader, queries, lanes < 200, emit),
        108 | 208 => vector_batch::<8, _, _>(reader, queries, lanes < 200, emit),
        1 => batch::<1, _, _>(reader, queries, emit),''')
replace_once('fn capture<', '''fn vector_batch<'a, const N: usize, B: AsRef<[u8]>, E: From<Error>>(
    reader: &'a Reader<B>, queries: &Queries, enable: bool,
    emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
) -> Result<(), E> {
    match queries {
        Queries::Ips(q) => reader.study_vector_ips::<N, E>(q, enable, emit),
        Queries::Asns(q) => reader.study_vector_asns::<N, E>(q, enable, emit),
    }
}
fn capture<''')
replace_once('"partial_and_empty_batches":true', '"partial_and_empty_batches":true,"forced_scalar_fallback":true,"cross_query_avx2":std::is_x86_feature_detected!("avx2")')
replace_once('    assert_eq!(reader.len(), records as usize);', '''    assert_eq!(reader.len(), records as usize);
    let build_started = Instant::now();
    let (cache_bytes,dimensions) = reader.study_prepare(epsilon)?;
    println!("{}", serde_json::json!({"phase":"cache", "build_ns":build_started.elapsed().as_nanos(),
        "allocation_bytes":cache_bytes,"budget_bytes":128*1024*1024,"dimensions":dimensions,"epsilon":epsilon}));''')
replace_once('        enum Stop {', '        reader.study_prepare(epsilon)?;\n        enum Stop {')
replace_once('    self_check()?;', '    let epsilon: usize = std::env::var("NETINDEX_STUDY_EPSILON")?.parse()?;\n    self_check(epsilon)?;')
replace_once('fn self_check() -> Result<(), Failure>', 'fn self_check(epsilon: usize) -> Result<(), Failure>')
replace_once('17, 33, 257', '17, 31, 32, 33, 257')
replace_once('                "duplicates-v4",', '                "duplicates-v4", "curved-v4", "curved-v6", "clustered-v4", "clustered-v6",')
replace_once('fn original_target(', 'fn learned_key(scenario:&str,id:u32)->u128 {\n    let n=u128::from(id);\n    let raw=if scenario.starts_with("curved") {n*n+n}else{((n/1024)<<26)+2*(n%1024)};\n    if scenario.contains("-v6-") {(u128::from(u64::MAX)<<64)+raw*((1u128<<32)+15)}else{raw}\n}\nfn original_target(')
replace_once('        "asn" => Target::Asn(id * 2 + 2),', '        s if s.starts_with("curved-") || s.starts_with("clustered-") => {\n            let value=learned_key(s,id);\n            Target::Address(if s.contains("-v6-") {Ipv6Addr::from(value).into()}else{Ipv4Addr::from(value as u32).into()})\n        },\n        "asn" => Target::Asn(id * 2 + 2),')
replace_once('            "asn" => asns.push(id * 2 + 2 + miss),', '            s if s.starts_with("curved-") || s.starts_with("clustered-") => {\n                let value=learned_key(s,id)+u128::from(miss);\n                ips.push(if s.contains("-v6-") {Ipv6Addr::from(value).into()}else{Ipv4Addr::from(value as u32).into()});\n            },\n            "asn" => asns.push(id * 2 + 2 + miss),')
replace_once('    if !asns.is_empty() {', '    if scenario.starts_with("clustered-") {\n        for cluster in 1..64 {\n            let a=learned_key(scenario,cluster*1024-1);\n            let b=learned_key(scenario,cluster*1024);\n            let value=a+(b-a)/2;\n            ips.push(if scenario.contains("-v6-") {Ipv6Addr::from(value).into()}else{Ipv4Addr::from(value as u32).into()});\n        }\n    }\n    if !asns.is_empty() {')
replace_once('        let original = oracle(scenario, records, &queries);', '        for method in [1,2] {\n            let stats=match &queries {Queries::Asns(q)=>reader.study_probe_asns(q,method)?,Queries::Ips(q)=>reader.study_probe_ips(q,method)?};\n            println!("{}",serde_json::json!({"phase":"probe","traffic":pattern,"method":700+method,"stats":stats}));\n        }\n        let original = oracle(scenario, records, &queries);')
shutil.copyfile(here/'fixtures.rs',output/'fixtures.rs')
# Generated-only field: ordinary constructors leave all research caches empty.
path = core / 'src/reader.rs'
text = path.read_text()
assert text.count('    broad_routes: Option<BroadRoutes>,') == 1
text = text.replace('    broad_routes: Option<BroadRoutes>,', '    broad_routes: Option<BroadRoutes>,\n    study_cache: std::sync::OnceLock<crate::study_learned::Cache>,')
assert text.count('            broad_routes,') == 1
text = text.replace('            broad_routes,', '            broad_routes,\n            study_cache: std::sync::OnceLock::new(),')
path.write_text(text, newline='\n')
(output / 'main.rs').write_text(main, newline='\n')
subprocess.run(['rustfmt', '+1.95.0', '--edition', '2024',
                str(output / 'main.rs'), str(core / 'src/lib.rs')], check=True)
receipt = dict(base_revision=subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
    original_sources={str(p.relative_to(repo)).replace('\\', '/'): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((repo/'src').glob('*.rs'))},
    prepared_sources={str(p.relative_to(output)).replace('\\', '/'): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(output.rglob('*')) if p.is_file()})
(output / 'snapshot.json').write_text(json.dumps(receipt, indent=2)+'\n', newline='\n')
print(output)
