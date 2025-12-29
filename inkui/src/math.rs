pub fn ceil_f64(x: f64) -> f64 {
    let xi = x as i64;
    if x > xi as f64 {
        (xi + 1) as f64
    } else {
        xi as f64
    }
}

pub fn min_f32(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

pub fn max_f32(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

pub fn sqrt_f64(x: f64) -> f64 {
    if x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 || x == f64::INFINITY {
        return x;
    }

    let mut _prev = 0.0; 
    let mut guess = x;

    for _ in 0..10 {
        _prev = guess; 
        guess = 0.5 * (guess + x / guess);

        if (guess - _prev).abs() < 1e-14 {
            break;
        }
    }

    guess
}

pub fn floor_f64(x: f64) -> f64 {
    let xi = x as i64;
    let xf = xi as f64;

    if xf > x {
        (xi - 1) as f64
    } else {
        xf
    }
}

pub fn ceil_f32(x: f32) -> f32 {
    let xi = x as i32;
    if x > xi as f32 {
        (xi + 1) as f32
    } else {
        xi as f32
    }
}

pub fn floor_f32(x: f32) -> f32 {
    let xi = x as i32;
    let xf = xi as f32;
    if xf > x {
        (xi - 1) as f32
    } else {
        xf
    }
}