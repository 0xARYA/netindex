"""Prepare an isolated P1-06 core and reuse the frozen P1-01 correctness harness."""
import hashlib
import json
import pathlib
import shutil
import subprocess

here = pathlib.Path(__file__).resolve().parent
repo = here.parents[1]
output = repo / 'target/adaptive'
core = output / 'library'
core.mkdir(parents=True, exist_ok=False)
shutil.copytree(repo / 'src', core / 'src')
shutil.copyfile(repo / 'README.md', core / 'README.md')
manifest = (repo / 'Cargo.toml').read_text().split('[dev-dependencies]')[0]
manifest = manifest.replace('name = "netindex"', 'name = "netindex-adaptive-core"\npublish = false', 1)
(core / 'Cargo.toml').write_text(manifest + '\n[workspace]\n', newline='\n')
for name in ['reader', 'packed']:
    path = core / f'src/{name}.rs'
    original = path.read_text()
    fragments = [(here.parent / 'interleaved' / f'{name}.inc.rs').read_text(), (here.parent / 'cross_query' / f'{name}.inc.rs').read_text(), (here / f'{name}.inc.rs').read_text()]
    patch = '\n'.join(fragments)
    marker = original.find('#[cfg(test)]')
    path.write_text(original + '\n' + patch if marker < 0 else original[:marker] + patch + '\n' + original[marker:], newline='\n')
shutil.copyfile(here.parent / 'cross_query/study_simd.rs', core / 'src/study_simd.rs')
bt = (here.parent/'flat_btree/study_flat_btree.rs').read_text()
start,end=bt.index('pub(crate) struct Cache {'),bt.index('#[cfg(test)]')
bt=bt[:start]+bt[end:]
bt=bt.replace('use crate::{Error, study_eytzinger::Tree as Eyt};','use crate::Error;')
bt=bt.replace('unsafe fn rank32(', '#[inline]\nunsafe fn rank32(').replace('unsafe fn rank128(', '#[inline]\nunsafe fn rank128(')
# Legacy kernels remain available for their independent unit oracles only.
for pattern in ['    fn vector_count(', '    fn ordered_rank(', '    pub(crate) fn bound(']:
    bt=bt.replace(pattern,'    #[cfg(test)]\n'+pattern)
for pattern in ['unsafe fn count32(', 'unsafe fn count128(']:
    bt=bt.replace(pattern,'#[cfg(test)]\n'+pattern)
(core/'src/study_flat_btree.rs').write_text(bt+'\n'+(here.parent/'traversal/btree.inc.rs').read_text(),newline='\n')
shutil.copyfile(here/'study_adaptive.rs',core/'src/study_adaptive.rs')
path=core/'src/lib.rs'
path.write_text(path.read_text()+'\nmod study_simd;\nmod study_flat_btree;\nmod study_adaptive;\n',newline='\n')
main = (here.parent / 'interleaved/src/main.rs').read_text()


def replace_once(old, new):
    global main
    assert main.count(old) == 1, old
    main = main.replace(old, new)


replace_once('const VARIANTS: [usize; 5] = [0, 1, 4, 8, 16];', 'const VARIANTS: [usize; 6] = [0, 108, 600, 601, 602, 603];')
# Forced scalar kernels participate in every full/partial/error precheck, not timing.
main = main.replace('for variant in VARIANTS {', 'for variant in VARIANTS.into_iter().chain([204, 208, 212, 604]) {')
replace_once('        1 => batch::<1, _, _>(reader, queries, emit),', """        603 => match queries {
            Queries::Asns(q) => {
                let mut emit = emit;
                for (chunk, values) in q.chunks(32).enumerate() {
                    reader.study_cached_asns(values, 2, |i, row| emit(chunk*32+i, row))?;
                }
                Ok(())
            }
            Queries::Ips(q) => {
                let mut emit = emit;
                for (chunk, values) in q.chunks(32).enumerate() {
                    reader.study_cached_ips(values, 2, |i, row| emit(chunk*32+i, row))?;
                }
                Ok(())
            }
        },
        600..=604 => match queries {
            Queries::Asns(q) => reader.study_cached_asns(q, lanes-600, emit),
            Queries::Ips(q) => reader.study_cached_ips(q, lanes-600, emit),
        },
        112 | 212 => match queries {
            Queries::Asns(q) => reader.study_vector_asns32(q, lanes < 200, emit),
            Queries::Ips(_) => vector_batch::<8, _, _>(reader, queries, lanes < 200, emit),
        },
        104 | 204 => vector_batch::<4, _, _>(reader, queries, lanes < 200, emit),
        108 | 208 => vector_batch::<8, _, _>(reader, queries, lanes < 200, emit),
        1 => batch::<1, _, _>(reader, queries, emit),""")
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
    let cache_bytes = reader.study_prepare(budget)?;
    println!("{}", serde_json::json!({"phase":"cache", "build_ns":build_started.elapsed().as_nanos(),
        "allocation_bytes":cache_bytes,"budget_bytes":budget}));''')
replace_once('        enum Stop {', '        reader.study_prepare(budget)?;\n        enum Stop {')
replace_once('    self_check()?;', '    let budget: usize = std::env::var("NETINDEX_STUDY_BUDGET")?.parse()?;\n    self_check(budget)?;')
replace_once('fn self_check() -> Result<(), Failure>', 'fn self_check(budget: usize) -> Result<(), Failure>')
replace_once('&["uniform", "hot"]', '&["uniform", "hot", "late-wide", "late-hot"]')
replace_once('let limit = if pattern == "hot" {', 'let local = pattern == "hot" || (pattern == "late-wide" && index < 32) || (pattern == "late-hot" && index >= 32);\n        let limit = if local {')
replace_once('17, 33, 257', '17, 31, 32, 33, 257')
replace_once('        let original = oracle(scenario, records, &queries);', """        let selected = match &queries {
            Queries::Asns(q) => reader.study_plan_asns(q)?,
            Queries::Ips(q) => reader.study_plan_ips(q)?,
        };
        println!("{}",serde_json::json!({"phase":"plan","traffic":pattern,"selected":selected}));
        let original = oracle(scenario, records, &queries);""")
# Generated-only field: ordinary constructors leave all research caches empty.
path = core / 'src/reader.rs'
text = path.read_text()
assert text.count('    broad_routes: Option<BroadRoutes>,') == 1
text = text.replace('    broad_routes: Option<BroadRoutes>,', '    broad_routes: Option<BroadRoutes>,\n    study_cache: std::sync::OnceLock<crate::study_adaptive::Cache>,')
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
