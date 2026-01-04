#ifndef _KRAKE_OS_H
#define _KRAKE_OS_H

#include <stddef.h>


#define EVENT_MOUSE 0
#define EVENT_KEYBOARD 1
#define EVENT_RESIZE 2
#define EVENT_REDRAW 3
#define EVENT_NONE 4


#define KEY_W 0x77
#define KEY_A 0x61
#define KEY_S 0x73
#define KEY_D 0x64
#define KEY_ENTER 0x0D
#define KEY_ESCAPE 0x1B
#define KEY_BACKSPACE 0x08
#define KEY_UP 0x110003
#define KEY_DOWN 0x110004
#define KEY_LEFT 0x110001
#define KEY_RIGHT 0x110002
#define KEY_CTRL 0x110005
#define KEY_ALT 0x110006
#define KEY_SHIFT 0x110007
#define KEY_SPACE 0x20

typedef struct {
    size_t id;
    size_t buffer; 
    unsigned long long pid;
    long long x, y;
    size_t z;
    size_t width, height;
    unsigned char can_move;
    unsigned char can_resize;
    unsigned char transparent;
    unsigned char treat_as_transparent;
    size_t min_width, min_height;
    size_t event_handler;
    int w_type;
} Window;

typedef struct {
    unsigned int type; 
    unsigned int arg1; 
    unsigned int arg2; 
    unsigned int arg3; 
    unsigned int arg4; 
} Event;

extern size_t krake_window_create(size_t width, size_t height, int transparent, int treat_as_transparent);
extern void krake_window_draw(size_t wid);
extern void* krake_window_get_buffer(size_t wid);
extern int krake_get_event(size_t wid, Event* out_event);
extern void krake_sleep(size_t ms);
extern size_t krake_get_time_ms();

#endif
