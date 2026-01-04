#ifndef _PWD_H
#define _PWD_H

#include <sys/types.h>

struct passwd {
    char *pw_name;
    char *pw_uid;
    char *pw_gid;
    char *pw_dir;
    char *pw_shell;
};

extern struct passwd *getpwuid(uid_t uid);
extern struct passwd *getpwent(void);
extern void endpwent(void);

#endif
