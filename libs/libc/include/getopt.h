#ifndef _GETOPT_H
#define _GETOPT_H

extern char *optarg;
extern int optind, opterr, optopt;

struct option {
    const char *name;
    int has_arg;
    int *flag;
    int val;
};

#define no_argument       0
#define required_argument 1
#define optional_argument 2

extern int getopt(int argc, char *const argv[], const char *optstring);
extern int getopt_long(int argc, char *const argv[], const char *optstring, const struct option *longopts, int *longindex);

#endif
