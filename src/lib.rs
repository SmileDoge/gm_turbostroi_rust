#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(deref_nullptr)]
#![allow(mutable_transmutes)]
#![allow(unused_must_use)]

// static VERSION: &str = env!("CARGO_PKG_VERSION");

use core::time;
use std::{mem::size_of, sync::{Arc, mpsc::{Sender, Receiver}, atomic::{AtomicBool, Ordering}, Mutex}, collections::HashMap, thread, ffi::CStr, ptr::{null_mut}, hint::unreachable_unchecked};

#[cfg(target_os="linux")]
use affinity::linux::set_affinity_mask;
#[cfg(target_os="linux")]
use affinity::linux::get_affinity_mask;

#[cfg(target_os="windows")]
use affinity::windows::set_affinity_mask;
#[cfg(target_os="windows")]
use affinity::windows::get_affinity_mask;

use bindings::{lua_settop, lua_getfield, lua_State, lua_setfield, LUA_GLOBALSINDEX, luaL_checkinteger, luaL_newmetatable, lua_setmetatable, lua_newuserdata, luaL_checkudata, lua_pushinteger, luaL_checklstring, lua_pushlstring, luaL_loadbuffer, lua_pcall, LUA_MULTRET, luaL_newstate, lua_pushboolean, lua_touserdata, luaL_openlibs, luaL_checknumber, lua_isstring, lua_error};
use libc::c_void;

use crate::bindings::{lua_createtable, lua_pushcclosure, lua_tolstring, lua_close, lua_pushlightuserdata, LUA_OK, lua_pushstring, lua_pushnumber};

#[macro_use]
extern crate lazy_static;

mod bindings;
mod defines;
mod affinity;


struct Train {
    pub id: i32,
    pub state: *mut lua_State,
    pub finished: Arc<AtomicBool>,
    pub to_gmod: Sender<Vec<u8>>,
    pub from_gmod: Receiver<Vec<u8>>,
}

struct SoftTrain {
    pub finished: Arc<AtomicBool>,
    pub to_thread: Sender<Vec<u8>>,
    pub from_thread: Receiver<Vec<u8>>,
}

lazy_static! {
    static ref trains : Mutex<HashMap<i32, SoftTrain>> = Mutex::default();
}

static mut targetTime: f32 = 0.0;
static mut rate: u64 = 100;

struct Msg {
    pub data: Option<Vec<u8>>,
    pub offset: usize,
}

method_define!(u8 );
method_define!(u16);
method_define!(u32);

method_define!(i8 );
method_define!(i16);
method_define!(i32);

method_define!(f32);

unsafe fn create_message_metatable(state: *mut lua_State) {
    if luaL_newmetatable(state, to_cstr!("tsmsg")) > 0 {

        luaL_newmetatable(state, to_cstr!("tsmsg"));
        lua_setfield(state, -2, to_cstr!("__index"));
        
        method_use!(state, u8 , "UInt8" );
        method_use!(state, u16, "UInt16");
        method_use!(state, u32, "UInt32");

        method_use!(state, i8 , "Int8" );
        method_use!(state, i16, "Int16");
        method_use!(state, i32, "Int32");

        method_use!(state, f32, "Float");

        lua_pushcclosure(state, Some(write_data), 0);
        lua_setfield(state, -2, to_cstr!("WriteData"));

        lua_pushcclosure(state, Some(read_data), 0);
        lua_setfield(state, -2, to_cstr!("ReadData"));

        lua_pushcclosure(state, Some(seek), 0);
        lua_setfield(state, -2, to_cstr!("Seek"));

        lua_pushcclosure(state, Some(msg_gc), 0);
        lua_setfield(state, -2, to_cstr!("__gc"));
    }
    lua_setmetatable(state, -2);
}

unsafe extern "C" fn read_data(state: *mut lua_State) -> i32 {
    let msg = to_type!(luaL_checkudata(state, 1, to_cstr!("tsmsg")), Msg);
    let len = luaL_checkinteger(state, 2) as usize;
    
    if let Some(data) = msg.data.as_ref() {
        let offset = msg.offset;
        let size_data = len;
        let size = data.len();
        
        let final_size = if size_data + offset > size { size - offset } else { size_data };
        
        msg.offset = msg.offset + final_size;
        
        lua_pushlstring(state, data.as_ptr().add(offset) as _, final_size as _);

        return 1;
    }

    0
}

unsafe extern "C" fn write_data(state: *mut lua_State) -> i32 {
    let msg = to_type!(luaL_checkudata(state, 1, to_cstr!("tsmsg")), Msg);

    if let Some(data) = msg.data.as_ref() {
        let mut size: u64 = 0;
        let str = luaL_checklstring(state, 2, &mut size);
        
        let offset = msg.offset;
        let size_data = size as usize;
        let size = data.len();

        let final_size = if size_data + offset > size { size - offset } else { size_data };

        str.copy_to(data.as_ptr().add(offset) as *mut i8, final_size);

        msg.offset = msg.offset + final_size;
    }

    0
}

unsafe extern "C" fn seek(state: *mut lua_State) -> i32 {
    let msg = to_type!(luaL_checkudata(state, 1, to_cstr!("tsmsg")), Msg);
    msg.offset = luaL_checkinteger(state, 2) as _;
    0
}

unsafe extern "C" fn tell(state: *mut lua_State) -> i32 {
    let msg = to_type!(luaL_checkudata(state, 1, to_cstr!("tsmsg")), Msg);
    lua_pushinteger(state, msg.offset as isize);
    1
}

unsafe extern "C" fn msg_gc(state: *mut lua_State) -> i32 {
    let msg = to_type!(luaL_checkudata(state, 1, to_cstr!("tsmsg")), Msg);
    msg.data = None;
    0
}

unsafe extern "C" fn create_message(state: *mut lua_State) -> i32 {
    let len = luaL_checkinteger(state, 1);

    // let msg: &mut Msg = &mut *(lua_newuserdata(state, size_of::<Msg>() as _) as *mut Msg);
    let msg = to_type!(lua_newuserdata(state, size_of::<Msg>() as _), Msg);
    msg.data = Some(vec![0; len as _]);
    msg.offset = 0;
    
    create_message_metatable(state);

    1
}

unsafe extern "C" fn send_message_gmod(state: *mut lua_State) -> i32 {
    let msg = to_type!(luaL_checkudata(state, 1, to_cstr!("tsmsg")), Msg);
    let id = luaL_checkinteger(state, 2) as i32;
    let mut data: Option<Vec<u8>> = None;

    std::mem::swap(std::mem::transmute(&msg.data), &mut data);

    if let Some(data) = data {
        if let Ok(lock) = trains.lock() {

            if let Some(train) = lock.get(&id) {
                train.to_thread.send(data);
            }
        }
    }

    0
}

unsafe extern "C" fn recv_message_gmod(state: *mut lua_State) -> i32 {
    let id = luaL_checkinteger(state, 1) as i32;

    if let Ok(lock) = trains.lock() {
        if let Some(train) = lock.get(&id) {
            if let Ok(data) = train.from_thread.try_recv() {
                let msg = to_type!(lua_newuserdata(state, size_of::<Msg>() as _), Msg);
                msg.data = Some(data);
                msg.offset = 0;

                create_message_metatable(state);

                return 1;
            }
        }
    }

    0
}

unsafe extern "C" fn send_message_train(state: *mut lua_State) -> i32 {
    let mut data: Option<Vec<u8>> = None;
    let msg = to_type!(luaL_checkudata(state, 1, to_cstr!("tsmsg")), Msg);

    lua_getglobal!(state, to_cstr!("__TRAIN"));
    let train = &mut *lua_touserdata(state, -1).cast::<Train>();
    lua_pop!(state, 1);

    std::mem::swap(std::mem::transmute(&msg.data), &mut data);
    
    if let Some(data) = data {
        train.to_gmod.send(data);
    }

    0
}

unsafe extern "C" fn recv_message_train(state: *mut lua_State) -> i32 {
    lua_getglobal!(state, to_cstr!("__TRAIN"));
    let train = &mut *lua_touserdata(state, -1).cast::<Train>();
    lua_pop!(state, 1);


    if let Ok(data) = train.from_gmod.try_recv() {
        let msg = to_type!(lua_newuserdata(state, size_of::<Msg>() as _), Msg);

        msg.data = Some(data);
        msg.offset = 0;

        create_message_metatable(state);

        return 1;
    }

    0
}

unsafe extern "C" fn load_string(state: *mut lua_State) -> i32 {
    let mut size: u64 = 0;
    let code = lua_tolstring(state, 1, &mut size);
    let mut name = to_cstr!("GMOD-LUA");

    if lua_isstring(state, 2) > 0 {
        name = lua_tolstring(state, 2, null_mut());
    }

    if luaL_loadbuffer(state, code, size, name) > 0 {
        lua_error(state);
        unreachable_unchecked();
    }

    1
}

unsafe extern "C" fn set_affinity_mask_lua(state: *mut lua_State) -> i32 {
    let mask = luaL_checkinteger(state, 1) as usize;
    set_affinity_mask(mask);
    0
}

unsafe extern "C" fn get_affinity_mask_lua(state: *mut lua_State) -> i32 {
    if let Some(mask) = get_affinity_mask() {
        lua_pushinteger(state, mask as _);
        return 1;
    }

    0
}

unsafe fn train_thread(train_ptr: *mut Train, code: &[u8]){
    let train = &mut *dbg!(train_ptr);
    let state = train.state;

    let now = std::time::Instant::now();

    /*
        state.globals().set("TURBOSTROI", true);
        state.globals().set("TRAIN_ID", train.id);
        state.globals().set("_TIME", targetTime);
        state.globals().set(
            "SysTime",
            state.create_function(move |_, ()| {
                Ok(now.elapsed().as_secs_f32())
            })?
        );
        state.globals().set(
            "loadstring", 
            state.create_function(|lua, (code, name): (String, Option<String>)| {
                let chunk = lua.load(code.as_ref());
                if let Some(name) = name{
                    let func = chunk.set_name(name.as_ref())?.into_function();
                    return func;
                }

                let func = chunk.into_function();
                return func;
            })?
        );
        state.globals().set(
            "CreateMessage",
            state.create_function(|_, size: usize| {
                if size > 16 * 1024 * 1024 {
                    return Err(format!("Maximum size 16 MB, Entered size: {} bytes", size))
                        .map_err(Error::external)?;
                }
                if size == 0 {
                    return Ok(None);
                }

                return Ok(Some(Msg {
                    data: Some(vec![0; size]),
                    offset: 0,
                }));
            })?
        );
        state.globals().set(
            "SendMessage", 
            state.create_function(move |_, ud: AnyUserData| {
                let mut data: Option<Vec<u8>> = None;
                let msg = ud.borrow::<Msg>()?;

                unsafe{
                    std::mem::swap(std::mem::transmute(&msg.data), &mut data);
                }

                if let Some(data) = data {
                    train.to_gmod.send(data);
                }

                Ok(())
            })?
        );
        state.globals().set(
            "RecvMessage",
            state.create_function(move |_, ()| {
                if let Ok(data) = train.from_gmod.try_recv() {
                    let msg = Msg{data: Some(data), offset: 0};
                    return Ok(Some(msg));
                }
                Ok(None)
            })?
        );
        state.globals().set(
            "SetAffinityMask",
            state.create_function(|_, mask: usize| {
                set_affinity_mask(mask);
                Ok(())
            })?
        );
        state.globals().set(
            "GetAffinityMask",
            state.create_function(|_, ()| {
                Ok(get_affinity_mask())
            })?
        );
    */
    
    lua_pushboolean(state, 1);
    lua_setglobal!(state, to_cstr!("TURBOSTROI"));

    lua_pushinteger(state, train.id as _);
    lua_setglobal!(state, to_cstr!("TRAIN_ID"));

    lua_pushlightuserdata(state, train_ptr as _);
    lua_setglobal!(state, to_cstr!("__TRAIN"));

    lua_pushcclosure(state, Some(load_string), 0);
    lua_setglobal!(state, to_cstr!("loadstring"));

    lua_pushcclosure(state, Some(create_message), 0);
    lua_setglobal!(state, to_cstr!("CreateMessage"));

    lua_pushcclosure(state, Some(send_message_train), 0);
    lua_setglobal!(state, to_cstr!("SendMessage"));

    lua_pushcclosure(state, Some(recv_message_train), 0);
    lua_setglobal!(state, to_cstr!("RecvMessage"));

    if luaL_loadbuffer(state, code.as_ptr() as _, code.len() as _, to_cstr!("sv_turbostroi_v3.lua"))>0 || lua_pcall(state, 0, LUA_MULTRET, 0)>0 {
        println!("Turbostroi error loading lua code! {}", from_cstr!(lua_tolstring(state, -1, null_mut())));
        
        lua_close(state);
        return ()
    }
    println!("LOAD BUFFER");
    // lua_getglobal!(state, to_cstr!("Think"));
    // let id = luaL_ref(state, LUA_REGISTRYINDEX);

    while !train.finished.load(Ordering::Relaxed) {
        // lua_rawgeti(state, LUA_REGISTRYINDEX, id);
        lua_getglobal!(state, to_cstr!("Think"));
        lua_pushnumber(state, 0.0);
        lua_pushnumber(state, now.elapsed().as_secs_f64());
        if lua_pcall(state, 2, LUA_MULTRET, 0) != LUA_OK as _ {
            println!("Turbostroi Think error! {}", from_cstr!(lua_tolstring(state, -1, null_mut())));
            lua_pop!(state, 1);
        }

        thread::sleep(time::Duration::from_millis(rate));
    }
    
    // luaL_unref(state, LUA_REGISTRYINDEX, id);
    lua_close(state);
    println!("END THREAD");

    ()
}


unsafe extern "C" fn initialize_train(state: *mut lua_State) -> i32 {
    let id = luaL_checkinteger(state, 1) as i32;

    let code = from_cstr!(luaL_checklstring(state, 2, null_mut()));

    if let Ok(mut lock) = trains.lock() {
        if let Some(_train) = lock.get(&id) {
            return 0;
        }

        let (to_gmod, from_thread) = std::sync::mpsc::channel();
        let (to_thread, from_gmod) = std::sync::mpsc::channel();
        let finished: Arc<AtomicBool> = Arc::default();
    
        lock.insert(id, SoftTrain { finished: finished.clone(), to_thread, from_thread });
    
        let raw_code = code.as_bytes();
    
        thread::spawn(move ||{
            unsafe {
                let L = luaL_newstate();
                luaL_openlibs(L);

                let mut train = Train{finished, id, state: L, to_gmod, from_gmod};
                train_thread( &mut train, raw_code);
            }
        });
    }
    
    0
}

unsafe extern "C" fn deinitialize_train(state: *mut lua_State) -> i32 {
    let id = luaL_checkinteger(state, 1) as i32;
    
    if let Ok(mut lock) = trains.lock() {
        if let Some(train) = lock.get(&id) {
            train.finished.swap(true, Ordering::Relaxed);
            lock.remove(&id);
        }
    }

    0
}

unsafe extern "C" fn set_fps_simulation(state: *mut lua_State) -> i32 {
    let target_rate = luaL_checknumber(state, 1) as u64; rate = target_rate; 0
}

unsafe extern "C" fn update_think(_: *mut lua_State) -> i32 {
    0
}

#[no_mangle]
unsafe extern "C" fn gmod13_open(state: *mut lua_State) -> i32 {
    lua_createtable(state, 0, 0);
        lua_pushstring(state, to_cstr!(env!("CARGO_PKG_VERSION")));
        lua_setfield(state, -2, to_cstr!("Version"));

        lua_pushcclosure(state, Some(create_message), 0);
        lua_setfield(state, -2, to_cstr!("CreateMessage"));

        lua_pushcclosure(state, Some(send_message_gmod), 0);
        lua_setfield(state, -2, to_cstr!("SendMessage"));

        lua_pushcclosure(state, Some(recv_message_gmod), 0);
        lua_setfield(state, -2, to_cstr!("RecvMessage"));

        lua_pushcclosure(state, Some(initialize_train), 0);
        lua_setfield(state, -2, to_cstr!("InitializeTrain"));

        lua_pushcclosure(state, Some(deinitialize_train), 0);
        lua_setfield(state, -2, to_cstr!("DeinitializeTrain"));

        lua_pushcclosure(state, Some(set_fps_simulation), 0);
        lua_setfield(state, -2, to_cstr!("SetFPSSimulation"));

        lua_pushcclosure(state, Some(update_think), 0);
        lua_setfield(state, -2, to_cstr!("UpdateThink"));
    lua_setglobal!(state, to_cstr!("Turbostroi"));

    0
}

#[no_mangle]
unsafe extern "C" fn gmod13_close(_: *mut c_void) -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "C" fn luaopen_io(_: *mut c_void) -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "C" fn luaopen_ffi(_: *mut c_void) -> i32 {
    0
}
