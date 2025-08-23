#/bin/bash
set -xe

FUZZ_TARGETS=$(find fuzz/fuzz_targets -name "*.rs")

objects=""
for target in $FUZZ_TARGETS; do
    basename=$(basename $target ".rs")
    cargo +nightly fuzz coverage $basename
    objects+="-object=target/aarch64-apple-darwin/coverage/aarch64-apple-darwin/release/$basename "
done

/opt/homebrew/opt/llvm/bin/llvm-profdata merge fuzz/coverage/**/*.profdata \
    -output fuzz/coverage/merged.profdata

/opt/homebrew/opt/llvm/bin/llvm-cov show \
    -instr-profile=fuzz/coverage/merged.profdata \
    -ignore-filename-regex=".cargo" \
    -ignore-filename-regex=".rustup" \
	-format=html \
	-output-dir=fuzz/coverage \
	$objects

printf "Report saved to fuzz/coverage/index.html\n"
