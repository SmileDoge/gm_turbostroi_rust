# $curDir = Get-Location

# $env:LUA_INC = $curDir.ToString() + "\external\luajit-src"
# $env:LUA_LIB = $curDir.ToString() + "\external"
# $env:LUA_LIB_NAME = "lua_shared"

# Write-Host "LUA_INC $env:LUA_INC"
# Write-Host "LUA_LIB $env:LUA_LIB"
# Write-Host "LUA_LIB_NAME $env:LUA_LIB_NAME"

# $env:LIBCLANG_PATH = "K:\LLVM\bin"

cargo build --release --target i686-pc-windows-msvc