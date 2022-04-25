#!/bin/bash

dir=$(pwd)

export LUA_INC="${dir}/external/luajit-src"
export LUA_LIB="${dir}/external"
export LUA_LIB_NAME="lua_shared_srv"

cargo build --release --target i686-unknown-linux-gnu
