use std::math::FloatMath;

const LN2: f64 = core::f64::consts::LN_2;
const INV_LN2: f64 = core::f64::consts::LOG2_E;




#[unsafe(no_mangle)]
pub unsafe extern "C" fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fabs(x: f64) -> f64 { x.abs() }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sin(x: f64) -> f64 { x.sin() }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cos(x: f64) -> f64 { x.cos() }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tan(x: f64) -> f64 { x.tan() }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atan(x: f64) -> f64 { x.atan() }
#[unsafe(no_mangle)]
pub extern "C" fn ceil(x: f64) -> f64 {
    x.ceil()
}


#[unsafe(no_mangle)]
pub extern "C" fn floor(x: f64) -> f64 {
    x.floor()
}


#[unsafe(no_mangle)]
pub extern "C" fn pow(x: f64, y: f64) -> f64 {
    x.powf(y)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn asin(_x: f64) -> f64 { 0.0 } 
#[unsafe(no_mangle)]
pub unsafe extern "C" fn acos(_x: f64) -> f64 { 0.0 } 

#[unsafe(no_mangle)]
pub unsafe extern "C" fn atan2(y: f64, x: f64) -> f64 { y.atan2(x) }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn log(_x: f64) -> f64 { 0.0 } 
#[unsafe(no_mangle)]
pub unsafe extern "C" fn log10(_x: f64) -> f64 { 0.0 } 
#[unsafe(no_mangle)]
pub unsafe extern "C" fn exp(_x: f64) -> f64 { 0.0 } 

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmod(x: f64, y: f64) -> f64 { x % y }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn frexp(x: f64, exp: *mut core::ffi::c_int) -> f64 {
    if x == 0.0 {
        *exp = 0;
        return 0.0;
    }
    let bits = x.to_bits();
    let e = ((bits >> 52) & 0x7FF) as i32;
    *exp = e - 1022;
    let mantissa = bits & 0x000FFFFFFFFFFFFF;
    f64::from_bits(mantissa | (1022 << 52))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ldexp(x: f64, exp: core::ffi::c_int) -> f64 {
    x * (2.0f64.powi(exp))
}




