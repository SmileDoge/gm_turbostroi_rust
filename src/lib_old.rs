#![allow(non_snake_case)]
#![allow(unused_must_use)]
#![allow(mutable_transmutes)]
#![allow(non_upper_case_globals)]

static VERSION: &str = env!("CARGO_PKG_VERSION");


#[cfg(target_os = "windows")]
use affinity::windows::{set_affinity_mask, get_affinity_mask};

#[cfg(target_os = "linux")]
use affinity::linux::{set_affinity_mask, get_affinity_mask};

use std::collections::HashMap;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{thread, time};
use std::{ffi::c_void, mem::size_of, os::raw::c_int};

use lua_shared as lua;
use lua_shared::lua_State;

#[macro_use]
extern crate lazy_static;

mod defines;
mod affinity;

struct Train {
    pub id: i32,
    pub state: Lua,
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

impl UserData for Msg {
    fn add_fields<'lua, F: mlua::UserDataFields<'lua, Self>>(_fields: &mut F) {}

    fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("ReadData", |state, this, len: usize| {
            if let Some(data) = this.data.as_ref() {
                let offset = this.offset;
                let size_data = len;
                let size = data.len();
                
                let final_size = if size_data + offset > size { size - offset } else { size_data };

                let ret = Ok(Some(state.create_string(&data[offset..final_size+offset])?));
                this.offset = this.offset + final_size;
                return ret;
            } else {
                return Ok(None);
            }
        });
        methods.add_method_mut(
            "WriteData",
            |_, this, val: mlua::String| {
                if let Some(data) = this.data.as_ref() {
                    let offset = this.offset;
                    let size_data = val.as_bytes().len();
                    let size = data.len();

                    let final_size = if size_data + offset > size { size - offset } else { size_data };

                    unsafe {
                        val.as_bytes().as_ptr().copy_to(
                            data.as_ptr().add(offset) as *mut u8,
                            final_size
                        );
                    }

                    this.offset = this.offset + final_size;
                }
                return Ok(());
            },
        );

        
        method_define!(methods, "Int8", i8);
        method_define!(methods, "Int16", i16);
        method_define!(methods, "Int32", i32);

        method_define!(methods, "UInt8", u8);
        method_define!(methods, "UInt16", u16);
        method_define!(methods, "UInt32", u32);

        method_define!(methods, "Float", f32);

        methods.add_method("Tell", |_, this, ()| {
            Ok(this.offset)
        });

        methods.add_method_mut("Seek", |_, this, pos: usize| {
            if let Some(data) = this.data.as_ref() {
                this.offset = pos.min(data.len());
            }

            Ok(())
        });

        // methods.add_method("Info", |_, this, (): ()| {
        //     let mut size = 0;

        //     if let Some(data) = this.data.as_ref() {
        //         size = data.len();
        //     }

        //     return Ok((this.id, size));
        // })
    }
}

unsafe fn train_thread(train: Train, code: &Vec<u8>) -> Result<()>{
    let state = train.state;

    let chunk = state.load(std::str::from_utf8_unchecked(code)).set_name("sv_turbostroi_v3.lua")?;

    let now = time::Instant::now();

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

    let res = chunk.exec();

    if let Err(_err) = res {
        println!("Turbostroi error loading lua code! (sv_turbostroi_v3.lua)");
        
        return Ok(());
    }

    while !train.finished.load(Ordering::Relaxed) {
        // let msg_data = train.from_gmod.try_recv().ok();

        // if let Some(msg) = msg_data {
        //     if let Ok(msgrecv) = state.globals().get::<_, Function>("OnMessageReceive") {
        //         msgrecv.call::<Msg,()>(Msg {id: 0, data: Some(msg)}); 
        //     }
        // }

        if let Ok(think) = state.globals().get::<_, Function>("Think") {
            think.call::<(f32, f32), ()>((targetTime, now.elapsed().as_secs_f32()));
        }

        thread::sleep(time::Duration::from_millis(rate));
    }
    
    Ok(())
}

#[no_mangle]
unsafe extern "C" fn gmod13_open(state: *mut c_void) -> i32 {
    let lua = Lua::init_from_ptr(state as _);

    fn initialize(lua: &Lua) -> Result<()> {
        let table = lua.create_table()?;

        table.set(
            "Version",
            VERSION
        );

        table.set(
            "CreateMessage",
            lua.create_function(|_, size: usize| {
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
            })?,
        );

        table.set(
            "InitializeTrain",
            lua.create_function(|_, (id, code): (i32, String)| {
                let mut lock = trains.lock().map_err(|err|err.to_string()).map_err(mlua::Error::external)?;

                if let Some(_train) = lock.get(&id) {
                    return Ok(());
                }

                let (to_gmod, from_thread) = std::sync::mpsc::channel();
                let (to_thread, from_gmod) = std::sync::mpsc::channel();
                let finished: Arc<AtomicBool> = Arc::default();

                lock.insert(id, SoftTrain { finished: finished.clone(), to_thread, from_thread });

                let raw_code = code.as_bytes().to_vec();

                thread::spawn(move ||{
                    unsafe {
                        let state = Lua::unsafe_new_with(StdLib::ALL ^ StdLib::PACKAGE, LuaOptions::default());

                        let train = Train{finished, id, state, to_gmod, from_gmod};
                        train_thread(train, &raw_code);
                    }
                });

                Ok(())
            })?
        );

        table.set(
            "DeinitializeTrain",
            lua.create_function(|_, id: i32| {
                let mut lock = trains.lock().map_err(|err|err.to_string()).map_err(mlua::Error::external)?;

                if let Some(train) = lock.get(&id) {
                    train.finished.swap(true, Ordering::Relaxed);
                    lock.remove(&id);
                }
                Ok(())
            })?
        );

        table.set(
            "SetFPSSimulation",
            lua.create_function(|_, targetRate: u64| {
                unsafe {
                    rate = targetRate;
                }
                Ok(())
            })?
        );

        table.set(
            "UpdateThink",
            lua.create_function(|_, time| {
                unsafe {
                    targetTime = time;
                }
                Ok(())
            })?
        );

        table.set(
            "SendMessage",
            lua.create_function(|_, (ud, id): (AnyUserData, i32)| {
                let mut data: Option<Vec<u8>> = None;
                let msg = ud.borrow::<Msg>()?;

                unsafe{
                    std::mem::swap(std::mem::transmute(&msg.data), &mut data);
                }

                if let Some(data) = data {
                    let lock = trains.lock().map_err(|err|err.to_string()).map_err(mlua::Error::external)?;

                    if let Some(train) = lock.get(&id) {
                        train.to_thread.send(data);
                    }
                }

                Ok(())
            })?
        );

        table.set(
            "RecvMessage",
            lua.create_function(|_, id: i32| {
                let lock = trains.lock().map_err(|err|err.to_string()).map_err(mlua::Error::external)?;

                if let Some(train) = lock.get(&id) {
                    if let Ok(data) = train.from_thread.try_recv() {
                        let msg = Msg{data: Some(data), offset: 0};
                        return Ok(Some(msg));
                    }
                }
                Ok(None)
            })?
        );

        lua.globals().set("Turbostroi", table);
        Ok(())
    }

    initialize(&lua).expect("Turbostroi: Failed initialize!");

    return 0;
}

#[no_mangle]
unsafe extern "C" fn gmod13_close(_: *mut c_void) -> i32 {
    return 0;
}

#[no_mangle]
pub unsafe extern "C" fn luaopen_io(_: *mut c_void) -> c_int {
    0
}
#[no_mangle]
pub unsafe extern "C" fn luaopen_ffi(_: *mut c_void) -> c_int {
    0
}
