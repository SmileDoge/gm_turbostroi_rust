#![allow(non_snake_case)]
#![allow(unused_must_use)]
#![allow(mutable_transmutes)]
#![allow(non_upper_case_globals)]

use lua_shared as lua;
use lua_shared::lua_State;
use std::{ffi::c_void, mem::size_of, sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc::{Sender, Receiver}, Mutex}, collections::HashMap, time};

use crate::affinity::windows::{set_affinity_mask};

#[macro_use]
extern crate lazy_static;

macro_rules! insert_function {
    ($state:ident, $name:expr, $func:expr) => {
        lua_shared::pushfunction($state, $func);
        lua_shared::setfield($state, -2, lua::cstr!($name));
    };
}
macro_rules! pushglobal {
    ($state:ident, $name:expr) => {
        lua_shared::setfield($state, lua::GLOBALSINDEX, lua::cstr!($name));
    };
}

mod defines;
mod affinity;

static VERSION: &str = env!("CARGO_PKG_VERSION");

static mut targetTime: f32 = 0.0;
static mut rate: u64 = 100;

struct Train {
    pub id: i32,
    pub state: lua_State,
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

struct Msg {
    pub data: Option<Vec<u8>>,
    pub offset: usize,
}

impl Msg {
    pub fn create_msg(state: lua_State) -> Result<i32, Box<dyn std::error::Error>> {
        unsafe {
            let size = lua::Lcheckinteger(state, 1) as usize;
            let msg = Self { data: Some(vec![0; size]), offset: 0 };
            let udata = lua::newuserdata(state, std::mem::size_of::<Msg>()).cast::<Msg>();
            udata.write(msg);
            Self::metatable(state);
            lua::setmetatable(state, -2);
        }
        Ok(1)
    }

    fn read_data(state: lua_State) -> Result<i32, Box<dyn std::error::Error>> {
        unsafe {
            let this = &mut *lua::Lcheckudata(state, 1, lua::cstr!("msgts")).cast::<Self>();
            let len = lua::Lcheckinteger(state, 2) as usize;

            if let Some(data) = this.data.as_ref() {
                let offset = this.offset;
                let size_data = len;
                let size = data.len();
                
                let final_size = if size_data + offset > size { size - offset } else { size_data };

                // let ret = Ok(Some(state.create_string(&data[offset..final_size+offset])?));
                let ret = &data[offset..final_size+offset];
                lua::pushlstring(state, ret.as_ptr(), ret.len());
                this.offset = this.offset + final_size;
                
                return Ok(1);
            }

            return Ok(0);
        }
    }
    
    fn write_data(state: lua_State) -> Result<i32, Box<dyn std::error::Error>> {
        unsafe {
            let this = &mut *lua::Lcheckudata(state, 1, lua::cstr!("msgts")).cast::<Self>();
            let size = &mut 0;
            let val = lua::Lchecklstring(state, 2, size);

            if let Some(data) = this.data.as_ref() {
                let offset = this.offset;
                let size_data = *size;
                let size = data.len();
        
                let final_size = if size_data + offset > size { size - offset } else { size_data };
        
                val.copy_to(
                    data.as_ptr().add(offset) as *mut u8,
                    final_size
                );
        
                this.offset = this.offset + final_size;
            }

            return Ok(0);
        }
    }

    method_define!("int8", i8);
    method_define!("int16", i16);
    method_define!("int32", i32);

    method_define!("uint8", u8);
    method_define!("uint16", u16);
    method_define!("uint32", u32);

    method_define!("float", f32);

    fn tell(state: lua_State) -> Result<i32, Box<dyn std::error::Error>> {
        unsafe {
            let this = &mut *lua::Lcheckudata(state, 1, lua::cstr!("msgts")).cast::<Self>();
            
            lua::pushinteger(state, this.offset as isize);

            return Ok(1);
        }
    }

    fn seek(state: lua_State) -> Result<i32, Box<dyn std::error::Error>> {
        unsafe {
            let this = &mut *lua::Lcheckudata(state, 1, lua::cstr!("msgts")).cast::<Self>();
            let pos = lua::Lcheckinteger(state, 2) as usize;

            if let Some(data) = this.data.as_ref() {
                this.offset = pos.clamp(0, data.len());
            }

            return Ok(0);
        }
    }

    fn __gc(state: lua_State) -> Result<i32, Box<dyn std::error::Error>> {
        unsafe {
            let _ = lua::Lcheckudata(state, 1, lua::cstr!("msgts"))
                .cast::<Self>()
                .read();
            Ok(0)
        }
    }

    fn metatable(state: lua_State){
        unsafe {
            if lua::Lnewmetatable(state, lua::cstr!("msgts")) {
                lua::pushvalue(state, -1);
                lua::setfield(state, -2, lua::cstr!("__index"));
                insert_function!(state, "__gc", Self::__gc);
                insert_function!(state, "ReadData", Self::read_data);
                insert_function!(state, "WriteData", Self::write_data);

                insert_function!(state, "ReadInt8", Self::read_int8);
                insert_function!(state, "ReadInt16", Self::read_int16);
                insert_function!(state, "ReadInt32", Self::read_int32);

                insert_function!(state, "WriteInt8", Self::write_int8);
                insert_function!(state, "WriteInt16", Self::write_int16);
                insert_function!(state, "WriteInt32", Self::write_int32);

                insert_function!(state, "ReadUInt8", Self::read_uint8);
                insert_function!(state, "ReadUInt16", Self::read_uint16);
                insert_function!(state, "ReadUInt32", Self::read_uint32);

                insert_function!(state, "WriteUInt8", Self::write_uint8);
                insert_function!(state, "WriteUInt16", Self::write_uint16);
                insert_function!(state, "WriteUInt32", Self::write_uint32);

                insert_function!(state, "ReadFloat", Self::read_float);
                insert_function!(state, "WriteFloat", Self::write_float);

                insert_function!(state, "Tell", Self::tell);
                insert_function!(state, "Seek", Self::seek);
            }
        }
    }
}

unsafe fn train_thread(train: Train, code: &Vec<u8>) -> Result<(), &'static str>{
    let state = train.state;
    
    let now = time::Instant::now();

    lua::pushboolean(state, 1);
    pushglobal!(state, "TURBOSTROI");

    lua::pushnumber(state, train.id as f64);
    pushglobal!(state, "TRAIN_ID");

    lua::pushnumber(state, targetTime as f64);
    pushglobal!(state, "_TIME");

    lua::pushfunction(state, move |state| {
        lua::pushnumber(state, now.elapsed().as_secs_f64());
        Ok(1)
    });
    pushglobal!(state, "SysTime");

    lua::pushfunction(state, |state| {
        let mut size =  0;
        let code = lua::Lchecklstring(state, 1, &mut size);

        let name = lua::Loptlstring(state, 2, lua::cstr!("loadstring-rust"), &mut 0);

        lua::Lloadbufferx(state, code, size, name, lua::cstr!("t"));

        Ok(1)
    });
    pushglobal!(state, "loadstring");

    lua::pushfunction(state, Msg::create_msg);
    pushglobal!(state, "CreateMessage");

    lua::pushfunction(state, move |state| {
        let mut data: Option<Vec<u8>> = None;
        let msg = &mut *lua::Lcheckudata(state, 1, lua::cstr!("msgts")).cast::<Msg>();

        unsafe{
            std::mem::swap(std::mem::transmute(&msg.data), &mut data);
        }

        if let Some(data) = data {
            train.to_gmod.send(data);
        }

        Ok(0)
    });
    pushglobal!(state, "SendMessage");

    lua::pushfunction(state, move |state| {
        if let Ok(data) = train.from_gmod.try_recv() {
            let msg = Msg{data: Some(data), offset: 0};
            let udata = lua::newuserdata(state, std::mem::size_of::<Msg>()).cast::<Msg>();
            udata.write(msg);
            Msg::metatable(state);
            lua::setmetatable(state, -2);
            return Ok(1);
        }
        Ok(0)
    });
    pushglobal!(state, "RecvMessage");

    lua::pushfunction(state, |state| {
        let mask = lua::Lcheckinteger(state, 1) as usize;
        set_affinity_mask(mask);

        Ok(0)
    });
    pushglobal!(state, "SetAffinityMask");

    // lua::pushfunction(state, |state| {
    //     if let Some(mask) = get_affinity_mask() {
    //         lua::pushinteger(state, mask as isize);
    //         return Ok(1);
    //     }

    //     return Ok(0);
    // });
    // pushglobal!(state, "GetAffinityMask");

    let status = lua::Lloadbufferx(state, code.as_ptr(), code.len(), lua::cstr!("sv_turbostroi_v3.lua"), lua::cstr!("t"));

    if let lua::Status::Ok = status {
        if let lua::Status::Ok = lua::pcall(state, 0, 0, 0) {

        }else{
            lua::close(state);
            return Ok(())
        }
    }

    while !train.finished.load(Ordering::Relaxed) {
        lua::getfield(state, lua::GLOBALSINDEX, lua::cstr!("Think"));

        lua::pushnumber(state, targetTime as f64);
        lua::pushnumber(state, now.elapsed().as_secs_f64());
        lua::pcall(state, 2, 0, 0);

        std::thread::sleep(time::Duration::from_millis(rate));
    }

    Ok(())
}

#[no_mangle]
unsafe extern "C" fn gmod13_open(state: *mut c_void) -> i32 {
    lua::createtable(state, 0, 0);

    lua::pushlstring(state, VERSION.as_ptr() as _, VERSION.as_bytes().len());
    lua::setfield(state, -2, lua::cstr!("Version"));

    insert_function!(state, "CreateMessage", Msg::create_msg);
    insert_function!(state, "InitializeTrain", |state| {
        let id = lua::Lcheckinteger(state, 1) as i32;
        let mut size = 0;
        let code = lua::Lchecklstring(state, 2, &mut size);

        let code_vec = Vec::from_raw_parts(code as *mut u8, size, size);

        let mut lock = trains.lock()?;

        if let Some(_train) = lock.get(&id) {
            return Ok(0);
        }
        
        let (to_gmod, from_thread) = std::sync::mpsc::channel();
        let (to_thread, from_gmod) = std::sync::mpsc::channel();
        let finished: Arc<AtomicBool> = Arc::default();

        lock.insert(id, SoftTrain { finished: finished.clone(), to_thread, from_thread });

        std::thread::spawn(move ||{
            unsafe {
                let state = lua::newstate();
                luaL_openlibs(state);

                let train = Train{finished, id, state, to_gmod, from_gmod};
                train_thread(train, &code_vec);
            }
        });

        return Ok(0);
    });
    insert_function!(state, "DeinitializeTrain", |state| {
        let id = lua::Lcheckinteger(state, 1) as i32;
        let mut lock = trains.lock()?;

        if let Some(train) = lock.get(&id) {
            train.finished.swap(true, Ordering::Relaxed);
            lock.remove(&id);
        }
        Ok(0)
    });
    insert_function!(state, "SetAffinityMask", |state| {
        let mask = lua::Lcheckinteger(state, 1) as usize;
        set_affinity_mask(mask);

        Ok(0)
    });
    // insert_function!(state, "GetAffinityMask", |state| {
    //     if let Some(mask) = get_affinity_mask() {
    //         lua::pushinteger(state, mask as isize);
    //         return Ok(1);
    //     }

    //     return Ok(0);
    // });
    insert_function!(state, "SetFPSSimulation", |state| {
        let target_rate = lua::Lcheckinteger(state, 1) as u64;
        rate = target_rate;
        
        Ok(0)
    });
    insert_function!(state, "UpdateThink", |state| {
        let time = lua::Lchecknumber(state, 1);
        targetTime = time as f32;
        
        Ok(0)
    });
    insert_function!(state, "SendMessage", |state| {
        let mut data: Option<Vec<u8>> = None;
        let msg = &mut *lua::Lcheckudata(state, 1, lua::cstr!("msgts")).cast::<Msg>();
        let id = lua::Lcheckinteger(state, 2) as i32;

        unsafe{
            std::mem::swap(std::mem::transmute(&msg.data), &mut data);
        }

        if let Some(data) = data {
            let lock = trains.lock()?;

            if let Some(train) = lock.get(&id) {
                train.to_thread.send(data);
            }
        }

        Ok(0)
    });
    insert_function!(state, "RecvMessage", |state| {
        let id = lua::Lcheckinteger(state, 1) as i32;

        let lock = trains.lock()?;

        if let Some(train) = lock.get(&id) {
            if let Ok(data) = train.from_thread.try_recv() {
                let msg = Msg{data: Some(data), offset: 0};
                let udata = lua::newuserdata(state, std::mem::size_of::<Msg>()).cast::<Msg>();
                udata.write(msg);
                Msg::metatable(state);
                lua::setmetatable(state, -2);
                return Ok(1);
            }
        }
        Ok(0)
    });

    lua::setfield(state, lua::GLOBALSINDEX, lua::cstr!("Turbostroi"));

    return 0;
}

#[no_mangle]
unsafe extern "C" fn gmod13_close(_: *mut c_void) -> i32 {
    return 0;
}

extern "C" {
    fn luaL_openlibs(state: *mut c_void);
}