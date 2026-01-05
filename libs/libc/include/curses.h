#ifndef _CURSES_H
#define _CURSES_H

#include <stdarg.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef unsigned long chtype;
typedef struct _win_st WINDOW;

extern int COLS;
extern int LINES;
extern WINDOW *curscr;
extern WINDOW *stdscr;

#define TRUE 1
#define FALSE 0
#define ERR -1
#define OK 0

/* Attributes */
#define A_NORMAL 0
#define A_REVERSE 1
#define A_BOLD 2
#define A_STANDOUT 4
#define A_UNDERLINE 8

/* Colors */
#define COLOR_BLACK 0
#define COLOR_RED 1
#define COLOR_GREEN 2
#define COLOR_YELLOW 3
#define COLOR_BLUE 4
#define COLOR_MAGENTA 5
#define COLOR_CYAN 6
#define COLOR_WHITE 7

#define COLOR_PAIR(n) (n << 8)

/* Keys */
#define KEY_DOWN 0x102
#define KEY_UP 0x103
#define KEY_LEFT 0x104
#define KEY_RIGHT 0x105
#define KEY_HOME 0x106
#define KEY_BACKSPACE 0x107
#define KEY_F0 0x108
#define KEY_F(n) (KEY_F0 + (n))
#define KEY_DL 0x148
#define KEY_IL 0x149
#define KEY_DC 0x14a
#define KEY_IC 0x14b
#define KEY_EIC 0x14c
#define KEY_CLEAR 0x14d
#define KEY_EOS 0x14e
#define KEY_EOL 0x14f
#define KEY_SF 0x150
#define KEY_SR 0x151
#define KEY_NPAGE 0x152
#define KEY_PPAGE 0x153
#define KEY_STAB 0x154
#define KEY_CTAB 0x155
#define KEY_CATAB 0x156
#define KEY_ENTER 0x157
#define KEY_SRESET 0x158
#define KEY_RESET 0x159
#define KEY_PRINT 0x15a
#define KEY_LL 0x15b
#define KEY_A1 0x15c
#define KEY_A3 0x15d
#define KEY_B2 0x15e
#define KEY_C1 0x15f
#define KEY_C3 0x160
#define KEY_BTAB 0x161
#define KEY_BEG 0x162
#define KEY_CANCEL 0x163
#define KEY_CLOSE 0x164
#define KEY_COMMAND 0x165
#define KEY_COPY 0x166
#define KEY_CREATE 0x167
#define KEY_END 0x168
#define KEY_EXIT 0x169
#define KEY_FIND 0x16a
#define KEY_HELP 0x16b
#define KEY_MARK 0x16c
#define KEY_MESSAGE 0x16d
#define KEY_MOVE 0x16e
#define KEY_NEXT 0x16f
#define KEY_OPEN 0x170
#define KEY_OPTIONS 0x171
#define KEY_PREVIOUS 0x172
#define KEY_REDO 0x173
#define KEY_REFERENCE 0x174
#define KEY_REFRESH 0x175
#define KEY_REPLACE 0x176
#define KEY_RESTART 0x177
#define KEY_RESUME 0x178
#define KEY_SAVE 0x179
#define KEY_SBEG 0x17a
#define KEY_SCANCEL 0x17b
#define KEY_SCOMMAND 0x17c
#define KEY_SCOPY 0x17d
#define KEY_SCREATE 0x17e
#define KEY_SDC 0x17f
#define KEY_SDL 0x180
#define KEY_SELECT 0x181
#define KEY_SEND 0x182
#define KEY_SEOL 0x183
#define KEY_SEXIT 0x184
#define KEY_SFIND 0x185
#define KEY_SHELP 0x186
#define KEY_SHOME 0x187
#define KEY_SIC 0x188
#define KEY_SLEFT 0x189
#define KEY_SMESSAGE 0x18a
#define KEY_SMOVE 0x18b
#define KEY_SNEXT 0x18c
#define KEY_SOPTIONS 0x18d
#define KEY_SPREVIOUS 0x18e
#define KEY_SPRINT 0x18f
#define KEY_SREDO 0x190
#define KEY_SREPLACE 0x191
#define KEY_SRIGHT 0x192
#define KEY_SRSUME 0x193
#define KEY_SSAVE 0x194
#define KEY_SSUSPEND 0x195
#define KEY_SUNDO 0x196
#define KEY_SUSPEND 0x197
#define KEY_UNDO 0x198
#define KEY_MOUSE 0x199
#define KEY_RESIZE 0x19a

/* Functions */
WINDOW *initscr(void);
int endwin(void);
int cbreak(void);
int noecho(void);
int nonl(void);
int keypad(WINDOW *win, bool bf);
int nodelay(WINDOW *win, bool bf);
int raw(void);
int beep(void);
int doupdate(void);
int wrefresh(WINDOW *win);
int wnoutrefresh(WINDOW *win);
int curs_set(int visibility);
int waddch(WINDOW *win, const chtype ch);
int mvwaddch(WINDOW *win, int y, int x, const chtype ch);
int waddstr(WINDOW *win, const char *str);
int mvwaddstr(WINDOW *win, int y, int x, const char *str);
int wmove(WINDOW *win, int y, int x);
int wclrtoeol(WINDOW *win);
int isendwin(void);
int wgetch(WINDOW *win);
int ungetch(int ch);
int napms(int ms);
int wscrl(WINDOW *win, int n);
int scrollok(WINDOW *win, bool bf);
int wattron(WINDOW *win, int attrs);
int wattroff(WINDOW *win, int attrs);
int mvwprintw(WINDOW *win, int y, int x, const char *fmt, ...);
int typeahead(int fd);
int wredrawln(WINDOW *win, int beg_line, int num_lines);
int waddnstr(WINDOW *win, const char *str, int n);
WINDOW *newwin(int nlines, int ncols, int begin_y, int begin_x);
int delwin(WINDOW *win);
int mvwaddnstr(WINDOW *win, int y, int x, const char *str, int n);

#ifdef __cplusplus
}
#endif

#endif
