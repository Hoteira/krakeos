#ifndef _MATH_H
#define _MATH_H

#define M_PI 3.14159265358979323846
#define HUGE_VAL (__builtin_huge_val())

extern double sqrt(double x);
extern double pow(double base, double exp);
extern double fabs(double x);
extern double sin(double x);
extern double cos(double x);
extern double tan(double x);
extern double atan(double x);
extern double ceil(double x);
extern double floor(double x);

extern double asin(double x);
extern double acos(double x);
extern double atan2(double y, double x);
extern double log(double x);
extern double log10(double x);
extern double exp(double x);
extern double fmod(double x, double y);
extern double frexp(double x, int *exp);
extern double ldexp(double x, int exp);

#endif
