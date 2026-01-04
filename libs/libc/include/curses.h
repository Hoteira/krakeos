#ifndef _CURSES_H
#define _CURSES_H

#include <stdint.h>
#include <stddef.h>

typedef struct {
    int _curr_y, _curr_x;
    int _max_y, _max_x;
} WINDOW;

#define OK  (0)
#define ERR (-1)

#define TRUE  1
#define FALSE 0

extern WINDOW *stdscr;
extern WINDOW *curscr;
extern int LINES, COLS;

extern WINDOW *initscr(void);
extern int endwin(void);
extern int isendwin(void);
extern int refresh(void);
extern int wrefresh(WINDOW *win);
extern int wnoutrefresh(WINDOW *win);
extern int doupdate(void);

extern int move(int y, int x);
extern int wmove(WINDOW *win, int y, int x);
extern int addch(int ch);
extern int waddch(WINDOW *win, int ch);
extern int addstr(const char *str);
extern int waddstr(WINDOW *win, const char *str);
extern int waddnstr(WINDOW *win, const char *str, int n);

extern int printw(const char *fmt, ...);
extern int wprintw(WINDOW *win, const char *fmt, ...);
extern int mvwprintw(WINDOW *win, int y, int x, const char *fmt, ...);

extern int getch(void);
extern int wgetch(WINDOW *win);
extern int nodelay(WINDOW *win, int bf);
extern int keypad(WINDOW *win, int bf);

extern int clear(void);
extern int wclear(WINDOW *win);
extern int clrtoeol(void);
extern int wclrtoeol(WINDOW *win);

extern int attron(int attrs);
extern int attroff(int attrs);
extern int wattron(WINDOW *win, int attrs);
extern int wattroff(WINDOW *win, int attrs);
extern int attrset(int attrs);
extern int wattrset(WINDOW *win, int attrs);

extern int standout(void);
extern int standend(void);

#define A_NORMAL    0
#define A_REVERSE   (1 << 8)
#define A_BOLD      (1 << 9)
#define A_DIM       (1 << 10)
#define A_UNDERLINE (1 << 11)

extern int start_color(void);
extern int has_colors(void);
extern int init_pair(short pair, short f, short b);
extern int COLOR_PAIR(int n);

#define COLOR_BLACK   0
#define COLOR_RED     1
#define COLOR_GREEN   2
#define COLOR_YELLOW  3
#define COLOR_BLUE    4
#define COLOR_MAGENTA 5
#define COLOR_CYAN    6
#define COLOR_WHITE   7

#define KEY_F0      0410
#define KEY_F(n)    (KEY_F0 + (n))
#define KEY_DOWN    0402
#define KEY_UP      0403
#define KEY_LEFT    0404
#define KEY_RIGHT   0405
#define KEY_HOME    0406
#define KEY_BACKSPACE 0407
#define KEY_NPAGE   0522
#define KEY_PPAGE   0523
#define KEY_END     0550
#define KEY_IC      0513
#define KEY_DC      0512
#define KEY_ENTER   0527
#define KEY_SLEFT   0611
#define KEY_SRIGHT  0622
#define KEY_A1      0534
#define KEY_C1      0544
#define KEY_A3      0551
#define KEY_C3      0552
#define KEY_SDC     0601
#define KEY_SCANCEL 0620
#define KEY_CANCEL  0621
#define KEY_SSUSPEND 0625
#define KEY_SUSPEND 0626
#define KEY_BTAB    0541
#define KEY_SBEG    0604
#define KEY_BEG     0542
#define KEY_B2      0543

extern int curs_set(int visibility);
extern int noecho(void);
extern int echo(void);
extern int raw(void);
extern int noraw(void);
extern int cbreak(void);
extern int nocbreak(void);
extern int nonl(void);
extern int typeahead(int fd);

extern int ungetch(int ch);

extern WINDOW *newwin(int nlines, int ncols, int begin_y, int begin_x);
extern int delwin(WINDOW *win);
extern int wborder(WINDOW *win, uint32_t ls, uint32_t rs, uint32_t ts, uint32_t bs, uint32_t tl, uint32_t tr, uint32_t bl, uint32_t br);
extern int wredrawln(WINDOW *win, int beg_line, int num_lines);
extern int scrollok(WINDOW *win, int bf);
extern int wscrl(WINDOW *win, int n);

extern int mvwaddstr(WINDOW *win, int y, int x, const char *str);
extern int mvwaddch(WINDOW *win, int y, int x, int ch);
extern int mvwaddnstr(WINDOW *win, int y, int x, const char *str, int n);

// For mouse support (dummy)
#define BUTTON1_RELEASED 0
#define BUTTON1_CLICKED 0
#define ALL_MOUSE_EVENTS 0
typedef struct { int x, y; int bstate; } MEVENT;
extern int getmouse(MEVENT *event);
extern int mousemask(int mask, int *oldmask);
extern int mouseinterval(int n);

extern int beep(void);
extern int napms(int ms);

#endif
