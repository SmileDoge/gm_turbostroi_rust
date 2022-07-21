
#[macro_export]
macro_rules! method_define {
    ($name:tt, $typ:ty) => {
        paste::paste! {
            fn [<read_ $name>](state: lua_State) -> Result<i32, Box<dyn std::error::Error>> {
                unsafe {
                    let this = &mut *lua::Lcheckudata(state, 1, lua::cstr!("msgts")).cast::<Self>();
                    
                    if let Some(data) = this.data.as_ref() {
                        let size = size_of::<$typ>();
        
                        if this.offset >= data.len() - size {return Ok(0);}
        
                        let res = *(data.as_ptr().add(this.offset) as *const $typ);
                        this.offset = this.offset + size;
                        lua::pushnumber(state, res as f64);
                        return Ok(1);
                    }

                    return Ok(0);
                }
            }
            fn [<write_ $name>](state: lua_State) -> Result<i32, Box<dyn std::error::Error>> {
                unsafe {
                    let this = &mut *lua::Lcheckudata(state, 1, lua::cstr!("msgts")).cast::<Self>();
                    let val = lua::Lchecknumber(state, 2) as $typ;

                    if let Some(data) = this.data.as_ref() {
                        let size = size_of::<$typ>();
                        
                        if this.offset >= data.len() - size {return Ok(0);}
    
                        *(data.as_ptr().add(this.offset) as *mut $typ) = val;
    
                        this.offset = this.offset + size_of::<$typ>();
                    }

                    return Ok(0);
                }
            }
        }
    };
}