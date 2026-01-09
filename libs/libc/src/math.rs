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




