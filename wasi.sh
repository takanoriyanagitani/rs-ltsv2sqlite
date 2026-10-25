#!/bin/sh

set -u

export CC_wasm32_wasip1="${WASI_SDK_PATH}/bin/clang"

test -x "${CC_wasm32_wasip1}" || exec env clng="${CC_wasm32_wasip1}" sh -c '
  echo invalid clang ${clng}.
  exit 1
'

cargo \
	build \
  --target wasm32-wasip1 \
  --profile release-wasi
