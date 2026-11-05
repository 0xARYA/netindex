"""Prepare an isolated P1-05 core and reuse the frozen P1-01 correctness harness."""
import hashlib
import json
import pathlib
import shutil
import subprocess

here = pathlib.Path(__file__).resolve().parent
repo = here.parents[1]
output = repo / 'target/traversal'
core = output / 'library'
core.mkdir(parents=True, exist_ok=False)
shutil.copytree(repo / 'src', core / 'src')
shutil.copyfile(repo / 'README.md', core / 'README.md')
manifest = (repo / 'Cargo.toml').read_text().split('[dev-dependencies]')[0]
manifest = manifest.replace('name = "netindex"', 'name = "netindex-traversal-core"\npublish = false', 1)
(core / 'Cargo.toml').write_text(manifest + '\n[workspace]\n', newline='\n')
for name in ['reader', 'packed']:
    path = core / f'src/{name}.rs'
    original = path.read_text()
    fragments = [(here.parent / 'interleaved' / f'{name}.inc.rs').read_text(), (here.parent / 'cross_query' / f'{name}.inc.rs').read_text(), (here / f'{name}.inc.rs').read_text()]
    patch = '\n'.join(fragments)
    marker = original.find('#[cfg(test)]')
    path.write_text(original + '\n' + patch if marker < 0 else original[:marker] + patch + '\n' + original[marker:], newline='\n')
shutil.copyfile(here.parent / 'cross_query/study_simd.rs', core / 'src/study_simd.rs')
bt = (here.parent / 'flat_btree/study_flat_btree.rs').read_text()
bt = bt.replace('unsafe fn rank32(', '#[inline]\nunsafe fn rank32(').replace('unsafe fn rank128(', '#[inline]\nunsafe fn rank128(')
bt = bt.replace('    pub(crate) b_asns: Tree<u32>,', '    pub(crate) mirror_asns: crate::study_eytzinger::Mirror<u32>,\n    pub(crate) mirror_v4: crate::study_eytzinger::Mirror<u128>,\n    pub(crate) mirror_v6: crate::study_eytzinger::Mirror<u128>,\n    pub(crate) b_asns: Tree<u32>,')
(core / 'src/study_flat_btree.rs').write_text(bt+'\n'+(here/'btree.inc.rs').read_text(), newline='\n')
eyt = (here.parent / 'eytzinger/study_eytzinger.rs').read_text()
start, end = eyt.index('pub(crate) struct Cache {'), eyt.index('#[cfg(test)]')
(core / 'src/study_eytzinger.rs').write_text(eyt[:start]+eyt[end:]+'\n'+(here/'eytzinger.inc.rs').read_text(), newline='\n')
path = core / 'src/lib.rs'
path.write_text(path.read_text() + '\nmod study_simd;\nmod study_flat_btree;\nmod study_eytzinger;\n', newline='\n')
main = (here.parent / 'interleaved/src/main.rs').read_text()


def replace_once(old, new):
    global main
    assert main.count(old) == 1, old
    main = main.replace(old, new)


replace_once('const VARIANTS: [usize; 5] = [0, 1, 4, 8, 16];', 'const VARIANTS: [usize; 17] = [0, 108, 300, 301, 302, 401, 404, 500, 501, 502, 503, 504, 505, 506, 507, 520, 521];')
# Forced scalar kernels participate in every full/partial/error precheck, not timing.
main = main.replace('for variant in VARIANTS {', 'for variant in VARIANTS.into_iter().chain([204, 208, 212, 403, 522]) {')
replace_once('        1 => batch::<1, _, _>(reader, queries, emit),', '''        500..=507 => match queries {
            Queries::Asns(q) => reader.study_traversal_asns(q, lanes-500, emit),
            Queries::Ips(q) => reader.study_traversal_ips(q, lanes-500, emit),
        },
        520..=522 => match queries {
            Queries::Asns(q) => reader.study_special_asns(q, lanes-520, emit),
            Queries::Ips(q) => reader.study_special_ips(q, lanes-520, emit),
        },
        400..=404 => match queries {
            Queries::Asns(q) => reader.study_btree_asns(q, lanes-400, emit),
            Queries::Ips(q) => reader.study_btree_ips(q, lanes-400, emit),
        },
        300..=302 => match queries {
            Queries::Asns(q) => reader.study_layout_asns(q, lanes-300, emit),
            Queries::Ips(q) => reader.study_layout_ips(q, lanes-300, emit),
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
    let cache_bytes = reader.study_prepare()?;
    println!("{}", serde_json::json!({"phase":"cache", "build_ns":build_started.elapsed().as_nanos(),
        "allocation_bytes":cache_bytes,"alignment_mod64":reader.study_alignment()?,"budget_bytes":512*1024*1024}));''')
replace_once('        enum Stop {', '        reader.study_prepare()?;\n        enum Stop {')
# Generated-only field: ordinary constructors leave all research caches empty.
path = core / 'src/reader.rs'
text = path.read_text()
assert text.count('    broad_routes: Option<BroadRoutes>,') == 1
text = text.replace('    broad_routes: Option<BroadRoutes>,', '    broad_routes: Option<BroadRoutes>,\n    study_cache: std::sync::OnceLock<crate::study_flat_btree::Cache>,')
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
