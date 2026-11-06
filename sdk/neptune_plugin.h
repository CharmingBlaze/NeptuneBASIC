/*
 * NeptuneBASIC plugin interface (version 1).
 *
 * A plugin is a shared library (.dll on Windows, .so on Linux, .dylib on
 * macOS) that exports neptune_plugin_init. Put it in a "plugins" folder next
 * to the neptune executable or next to your program (or in a folder listed in
 * NEPTUNE_PLUGIN_PATH). Its commands become ordinary BASIC commands: the
 * compiler checks their arguments, editors complete them, and `neptune build`
 * copies the plugin next to the packaged game.
 *
 * Command signatures use Blitz3D's notation: an optional leading return type
 * tag, then the name, then each parameter with its type tag and an optional
 * default value.
 *     "%AddInts%a%b"          Function AddInts%(a%, b%)
 *     "$Greet$name"           Function Greet$(name$)
 *     "#Lerp#a#b#t=0.5"       Function Lerp#(a#, b#, t# = 0.5)
 *     "Beep%times=1"          a command with no return value
 * Tags: % int, # float, $ string, ? any value.
 *
 * Arguments arrive already converted to the declared types. Strings are UTF-8
 * and are valid only during the call. A string you return must stay valid
 * until your plugin's next command call (a static or plugin-owned buffer).
 *
 * Return 0 from a command on success. Return non-zero to raise a runtime
 * error; put the message in `result` as a string (programs can catch it with
 * Try/Catch).
 */
#ifndef NEPTUNE_PLUGIN_H
#define NEPTUNE_PLUGIN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define NEPTUNE_PLUGIN_API_VERSION 1

#define NB_NULL 0
#define NB_INT 1
#define NB_FLOAT 2
#define NB_STRING 3

typedef struct NbValue {
    int32_t kind;      /* NB_NULL, NB_INT, NB_FLOAT or NB_STRING */
    int32_t i;         /* the value when kind is NB_INT (also set for floats, truncated) */
    float f;           /* the value when kind is NB_FLOAT (also set for ints) */
    const char *s;     /* UTF-8 text when kind is NB_STRING */
    size_t len;        /* length of s in bytes (0 = NUL-terminated) */
} NbValue;

typedef int32_t (*NbCommand)(void *user, const NbValue *args, int32_t argc, NbValue *result);

typedef struct NbHost {
    uint32_t api_version;
    void *host;
    /* returns 0 if the command was added (non-zero: bad signature or the name is taken) */
    int32_t (*add_command)(void *host, const char *signature, NbCommand command, void *user);
    void (*add_constant_int)(void *host, const char *name, int32_t value);
    void (*add_constant_float)(void *host, const char *name, float value);
    void (*add_constant_string)(void *host, const char *name, const char *value);
} NbHost;

#if defined(_WIN32)
#define NEPTUNE_EXPORT __declspec(dllexport)
#else
#define NEPTUNE_EXPORT __attribute__((visibility("default")))
#endif

/* Your plugin defines this; return 0 on success. */
NEPTUNE_EXPORT int32_t neptune_plugin_init(const NbHost *host);

#ifdef __cplusplus
}
#endif

#endif /* NEPTUNE_PLUGIN_H */
