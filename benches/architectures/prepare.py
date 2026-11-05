"""Generate an isolated architecture study; never modify production sources."""
import hashlib
import json
import pathlib
import shutil
import subprocess

here = pathlib.Path(__file__).resolve().parent
repo = here.parents[1]
out = repo / 'target/architectures'
core = out / 'library'
core.mkdir(parents=True, exist_ok=False)
shutil.copytree(repo / 'src', core / 'src')
shutil.copyfile(repo / 'README.md', core / 'README.md')
manifest = (repo / 'Cargo.toml').read_text().split('[dev-dependencies]')[0]
manifest = manifest.replace('name = "netindex"', 'name = "netindex-architectures-core"\npublish = false', 1)
(core / 'Cargo.toml').write_text(manifest + '\n[workspace]\n', newline='\n')
for name in ['reader', 'packed']:
    path = core / f'src/{name}.rs'
    source = path.read_text()
    fragment = (here.parent / 'interleaved' / f'{name}.inc.rs').read_text()
    marker = source.find('#[cfg(test)]')
    if marker < 0:
        marker = len(source)
    path.write_text(source[:marker] + fragment + '\n' + source[marker:], newline='\n')

path = core / 'src/search.rs'
source = path.read_text().replace('pub(crate) struct Search {', 'pub(crate) struct Search {\n    study_mode: u8,')
source = source.replace('        Self {\n', '        Self {\n            study_mode: 0,\n', 1)
source = source.replace('let scalar = Search {', 'let scalar = Search {\n            study_mode: 1,')
marker = '        #[cfg(all(feature = "simd", target_arch = "x86_64"))]\n        if self.avx2'
assert source.count(marker) == 1
source = source.replace(marker, '''        match self.study_mode {
            1 => return binary(bytes, width, target),
            2 | 3 | 4 => return crate::study_arch::upper(bytes, width, target, self.study_mode),
            _ => {}
        }
''' + marker)
source += '''
impl Search {
    pub(crate) fn study(mode: u8) -> Self {
        Self { study_mode: mode, ..Self::detect() }
    }
}
'''
path.write_text(source, newline='\n')
path = core / 'src/reader.rs'
source = path.read_text()
start = source.index('    pub fn visit_ip<')
end = source.index('    /// Visit every assertion for an exact ASN', start)
method = source[start:end].replace('pub fn visit_ip', 'pub fn study_arch_ip', 1)
method = method.replace('        address: IpAddr,', '        address: IpAddr,\n        mode: u8,', 1)
method = method.replace('                self.search,', '                crate::search::Search::study(mode),')
source += '\nimpl<B: AsRef<[u8]>> Reader<B> {\n' + method + '}\n'
path.write_text(source, newline='\n')
shutil.copyfile(here / 'study_arch.rs', core / 'src/study_arch.rs')
path = core / 'src/lib.rs'
path.write_text(path.read_text() + '\nmod study_arch;\npub use study_arch::{study_available, study_kernel};\n', newline='\n')

main = (here.parent / 'interleaved/src/main.rs').read_text()
main = main.replace('const VARIANTS: [usize; 5] = [0, 1, 4, 8, 16];', '''#[cfg(target_arch = "x86_64")]
const VARIANTS: [usize; 4] = [0, 8, 800, 801];
#[cfg(target_arch = "aarch64")]
const VARIANTS: [usize; 5] = [0, 8, 800, 802, 803];
mod kernels;
''')
main = main.replace('    let args: Vec<_> = std::env::args().collect();', '''    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("kernels") {
        pin()?;
        return kernels::run();
    }
    if args.get(1).map(String::as_str) == Some("fixtures") {
        return kernels::fixtures(&args[2]);
    }''')
main = main.replace('    if lanes == 0 {', '    if lanes == 0 || lanes >= 800 {', 1)
main = main.replace('                    reader.visit_ip(q, |row| emit(index, row))?;', '''                    if lanes >= 800 {
                        reader.study_arch_ip(q, (lanes - 799) as u8, |row| emit(index, row))?;
                    } else {
                        reader.visit_ip(q, |row| emit(index, row))?;
                    }''', 1)
# Exercise the unavailable architecture's portable fallback in every precheck too.
main = main.replace('for variant in VARIANTS {', 'for variant in VARIANTS.into_iter().chain([800, 801, 802, 803]) {')
main = main.replace('libc::CPU_SET(20, &mut set);', '''let cpu: usize = std::env::var("NETINDEX_CPU").unwrap_or_else(|_| "20".into()).parse().map_err(std::io::Error::other)?;
        if cpu >= libc::CPU_SETSIZE as usize { return Err(std::io::Error::other("CPU outside mask")); }
        libc::CPU_SET(cpu, &mut set);''')
main = main.replace('CPU 20 fits the initialized mask', 'The requested CPU is checked against the initialized mask')
(out / 'main.rs').write_text(main, newline='\n')
shutil.copyfile(here / 'kernels.rs', out / 'kernels.rs')
files = list((core / 'src').glob('*.rs')) + [out / 'main.rs', out / 'kernels.rs']
subprocess.run(['rustfmt', '+1.95.0', '--edition', '2024', *map(str, files)], check=True)
receipt = {'base_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
           'original': {p.relative_to(repo).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((repo / 'src').glob('*.rs'))},
           'prepared': {p.relative_to(out).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file()}}
(out / 'prepared.json').write_text(json.dumps(receipt, indent=2) + '\n')
shutil.copyfile(here / 'Cargo.lock', core / 'Cargo.lock')
