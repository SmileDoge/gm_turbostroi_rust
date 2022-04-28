

#[macro_export]
macro_rules! lua_pop {
    ($L:expr, $n:expr) => {
        $crate::lua_settop($L, -($n) - 1)
    };
}

#[macro_export]
macro_rules! lua_getglobal {
    ($L:expr, $s:expr) => {
        $crate::lua_getfield($L, $crate::LUA_GLOBALSINDEX, $s)
    };
}

#[macro_export]
macro_rules! lua_setglobal {
    ($L:expr, $s:expr) => {
        $crate::lua_setfield($L, $crate::LUA_GLOBALSINDEX, $s)
    };
}

#[macro_export]
macro_rules! to_cstr {
    ($s:expr) => {
        concat!($s, "\0").as_ptr() as _
    };
}

#[macro_export]
macro_rules! from_cstr {
    ($s:expr) => {
        std::str::from_utf8_unchecked(CStr::from_ptr($s).to_bytes())
    };
}

#[macro_export]
macro_rules! to_type {
    ($a:expr, $typ:ty) => {
        &mut *($a as *mut $typ)
    }
}

#[macro_export]
macro_rules! method_define {
    ($typ:ty) => {
        paste::paste! {
            unsafe extern "C" fn [<read_$typ>](state: *mut lua_State) -> i32 {
                let msg = to_type!(luaL_checkudata(state, 1, to_cstr!("tsmsg")), Msg);
                
                if let Some(data) = msg.data.as_ref() {
                    let size = size_of::<$typ>();

                    if msg.offset >= data.len() - size {return 0;}

                    let res = *(data.as_ptr().add(msg.offset) as *const $typ);

                    msg.offset = msg.offset + size;
                    lua_pushinteger(state, res as _);

                    return 1;
                } else {
                    return 0;
                }
            }

            unsafe extern "C" fn [<write_$typ>](state: *mut lua_State) -> i32 {
                let msg = to_type!(luaL_checkudata(state, 1, to_cstr!("tsmsg")), Msg);
                let val = luaL_checkinteger(state, 2) as $typ;

                if let Some(data) = msg.data.as_ref() {
                    let size = size_of::<$typ>();
                    
                    if msg.offset >= data.len() - size {return 0}

                    *(data.as_ptr().add(msg.offset) as *mut $typ) = val;

                    msg.offset = msg.offset + size_of::<$typ>();
                }

                0
            }

        }
    };
}

#[macro_export]
macro_rules! method_use {
    ($state:ident, $typ:ty, $func_name:tt) => {
        paste::paste! {
            lua_pushcclosure($state, Some([<write_$typ>]), 0);
            lua_setfield($state, -2, to_cstr!(concat!("Write", $func_name)));
            lua_pushcclosure($state, Some([<read_$typ>]), 0);
            lua_setfield($state, -2, to_cstr!(concat!("Read", $func_name)));
        }
    };
}