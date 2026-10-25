#!/bin/bash

wsm="./ltsv2sqlite.wasm"

hdir="$PWD"
gdir="/guest.d"
bname="example.sqlite.db3"
gpath="${gdir}/${bname}"
hpath="${hdir}/${bname}"

run_wasmtime(){
  cat /dev/stdin |
    wasmtime \
      run \
      --env ENV_SQLITE_DB_FILE_PATH="${gpath}" \
      --dir "${hdir}::${gdir}" \
      "${wsm}"
}

run_wasi(){
  run_wasmtime
}

input1(){
  printf 'timestamp:%s\tseverity:%s\tcategory:42\tmessage:%s\n' \
    "2026-10-21T03:09:47.012345Z" \
    "INFO" \
    "hello world"
}

input2(){
  printf 'timestamp:%s\tseverity:%s\tcategory:42\tmisc:ok\tmessage:%s\n' \
    "2025-10-21T03:09:47.012345Z" \
    "WARN" \
    "apt update failure"
}

input(){
  input1
  input2
}

echo converting to sqlite...
input | run_wasmtime
echo

echo printing logs...
echo "
  SELECT * FROM ltsv ORDER BY rowid
" | sqlite3 \
  -markdown \
  "${hpath}"
