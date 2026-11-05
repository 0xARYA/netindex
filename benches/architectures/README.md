# P1-08: native architecture qualification

Isolated research, with no change to library code, public APIs or encoding.
Requires Linux x86-64 with AVX-512F/BW, or little-endian Linux AArch64 with NEON.
Rust 1.95.0, rustfmt, Clippy, Python 3 and objdump must be installed.

From a fresh checkout:

```sh
python3 benches/architectures/prepare.py
python3 benches/architectures/run.py --output /tmp/native-study --fixtures /tmp/native-fixtures
```

Select a permitted CPU with `--cpu`; otherwise the last permitted CPU is used.
The output directory must be fresh; fixtures are created when missing and can
be reused. Each file is privately copied
before mapping. Two separate processes repeat every case, each with four
alternating-order trials and a per-method warmup. No PGO, custom target flags,
LTO, cache flushing or page-dropping is used. Results describe warm operation.

Variants: 0 ordinary visitor, 8 existing interleaving, 800 scalar packed search,
801 AVX-512 tail, 802 NEON byte-mask scan, 803 NEON packed-mask tail. The host-native variants are timed; both native
and unavailable-ISA fallback variants participate in correctness checks.
ASN and overlapping targets retain existing lookup paths. No cache is added.
The AVX-512 tail uses unsigned mask comparisons for widths 1/2/4/8, and paired
64-bit comparisons for width 16. NEON uses unsigned comparisons for widths
1/2/4/8 and a scalar fallback for width 16. The packed-mask version narrows
comparison bytes into four-bit masks, then uses trailing-zero count to select
the first greater element. The generated visitor changes only
the packed-search argument. Full targets, IDs and borrowed payloads are checked
against original input formulas before any timing; mixed families, duplicate
records, broad routes, empty/partial input and callback failures are checked too.

Kernel checks cover all 64 byte alignments, short columns, vector boundaries,
unsigned limits, duplicate values and malformed widths/lengths. Synthetic
kernel timings include widths 1/2/4/8/16 and 32 through 1,048,576 entries;
small-width synthetic columns can contain repeated values. These timings alone
do not establish an end-to-end benefit. The 16 end-to-end fixtures are the
unchanged six 1M/20M exact-key and ten shared/unique shape corpora from earlier
P1 studies, reconstructed from their frozen input formulas.

Raw JSONL, host/toolchain details, disassembly, input bytes, executable and SHA-256
receipts remain under the output directory. The GitHub Actions workflow runs
the NEON study on `ubuntu-24.04-arm`, asserts runtime NEON availability, and
uploads logs and receipts. Runner timings are observations on a virtualized
host; compare controls within a host, not raw times between hosts.

Native qualification is complete; see [results and limitations](RESULTS.md).
Validate and summarize retained evidence with:

```sh
python3 benches/architectures/report.py /tmp/native-study --output /tmp/measurements.json
python3 benches/architectures/verify_source.py /tmp/native-study
python3 benches/architectures/inspect_asm.py /tmp/native-study --output /tmp/instructions.json
```

With the earlier `target/traffic-fixtures` and `target/shape-fixtures` retained,
`verify_inputs.py NATIVE_OUTPUT [SECOND_NATIVE_OUTPUT]` also checks that both
hosts used precisely the earlier P1 bytes. Reproduce against the study revision
in the results; later production-source changes invalidate its snapshot.
