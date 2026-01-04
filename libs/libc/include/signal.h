#ifndef _SIGNAL_H
#define _SIGNAL_H

#include <sys/types.h>

#define SIGHUP  1
#define SIGINT  2
#define SIGQUIT 3
#define SIGILL  4
#define SIGTRAP 5
#define SIGABRT 6
#define SIGBUS  7
#define SIGFPE  8
#define SIGKILL 9
#define SIGUSR1 10
#define SIGSEGV 11
#define SIGUSR2 12
#define SIGPIPE 13
#define SIGALRM 14
#define SIGTERM 15

typedef void (*sighandler_t)(int);

typedef unsigned long sigset_t;

struct sigaction {
    sighandler_t sa_handler;
    sigset_t sa_mask;
    int sa_flags;
};

#define SA_RESETHAND 0x80000000

extern int kill(pid_t pid, int sig);
extern sighandler_t signal(int signum, sighandler_t handler);
extern int sigaction(int signum, const struct sigaction *act, struct sigaction *oldact);
extern int sigemptyset(sigset_t *set);
extern int sigaddset(sigset_t *set, int signum);
extern int sigfillset(sigset_t *set);
extern int sigprocmask(int how, const sigset_t *set, sigset_t *oldset);

#define SIG_BLOCK 0
#define SIG_UNBLOCK 1
#define SIG_SETMASK 2

#define SIG_IGN ((sighandler_t)1)
#define SIG_DFL ((sighandler_t)0)

#endif
