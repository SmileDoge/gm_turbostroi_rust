
#[macro_export]
macro_rules! method_define {
    ($methods:ident, $name:tt, $typ:ty) => {
        $methods.add_method_mut(concat!("Read", $name), |_, this, ()| {
            if let Some(data) = this.data.as_ref() {
                let size = size_of::<$typ>();

                if this.offset >= data.len() - size {return Ok(None);}

                unsafe {
                    let res = Ok(Some(*(data.as_ptr().add(this.offset) as *const $typ)));
                    this.offset = this.offset + size;
                    return res;
                }
            } else {
                return Ok(None);
            }
        });
        $methods.add_method_mut(
            concat!("Write", $name),
            |_, this, val: $typ| {
                if let Some(data) = this.data.as_ref() {
                    let size = size_of::<$typ>();
                    
                    if this.offset >= data.len() - size {return Ok(());}

                    unsafe {
                        *(data.as_ptr().add(this.offset) as *mut $typ) = val;
                    }

                    this.offset = this.offset + size_of::<$typ>();
                }
                return Ok(());
            },
        );
    };
}