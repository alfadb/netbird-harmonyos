#include <dlfcn.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <napi/native_api.h>

#define G0_LOADER_ERROR_MAX 512

typedef int (*hello_fn)(void);
typedef long long (*runtime_fn)(long long);

static void SanitizeLoaderError(char *message)
{
    if (message == NULL) {
        return;
    }
    for (unsigned char *cursor = (unsigned char *)message; *cursor != '\0'; ++cursor) {
        if (*cursor < 0x20 || *cursor == 0x7f || *cursor == (unsigned char)'|') {
            *cursor = (unsigned char)' ';
        }
    }
}

static void CaptureLoaderError(char *buffer, size_t buffer_size)
{
    const char *error = dlerror();
    if (error == NULL) {
        error = "(null)";
    }
    strncpy(buffer, error, buffer_size - 1);
    buffer[buffer_size - 1] = '\0';
    SanitizeLoaderError(buffer);
}

static napi_value BuildResult(napi_env env, bool ok, const char *stage, int loader_errno,
    const char *loader_error, int hello, long long runtime_bytes)
{
    napi_value result = NULL;
    napi_value ok_value = NULL;
    napi_value stage_value = NULL;
    napi_value errno_value = NULL;
    napi_value error_value = NULL;
    napi_value hello_value = NULL;
    napi_value bytes_value = NULL;
    napi_value pid_value = NULL;

    if (napi_create_object(env, &result) != napi_ok ||
        napi_get_boolean(env, ok, &ok_value) != napi_ok ||
        napi_create_string_utf8(env, stage, NAPI_AUTO_LENGTH, &stage_value) != napi_ok ||
        napi_create_int32(env, loader_errno, &errno_value) != napi_ok ||
        napi_create_string_utf8(env, loader_error, NAPI_AUTO_LENGTH, &error_value) != napi_ok ||
        napi_create_int32(env, hello, &hello_value) != napi_ok ||
        napi_create_int64(env, (int64_t)runtime_bytes, &bytes_value) != napi_ok ||
        napi_create_int32(env, (int32_t)getpid(), &pid_value) != napi_ok ||
        napi_set_named_property(env, result, "ok", ok_value) != napi_ok ||
        napi_set_named_property(env, result, "stage", stage_value) != napi_ok ||
        napi_set_named_property(env, result, "loaderErrno", errno_value) != napi_ok ||
        napi_set_named_property(env, result, "loaderError", error_value) != napi_ok ||
        napi_set_named_property(env, result, "hello", hello_value) != napi_ok ||
        napi_set_named_property(env, result, "runtimeBytes", bytes_value) != napi_ok ||
        napi_set_named_property(env, result, "pid", pid_value) != napi_ok) {
        napi_throw_error(env, NULL, "failed to build go probe result");
        return NULL;
    }
    return result;
}

static napi_value RunGoProbe(napi_env env, napi_callback_info info)
{
    size_t argc = 0;
    napi_value argv[1] = { NULL };
    if (napi_get_cb_info(env, info, &argc, argv, NULL, NULL) != napi_ok || argc != 0) {
        napi_throw_error(env, NULL, "runGoProbe takes no arguments");
        return NULL;
    }

    errno = 0;
    dlerror();
    void *handle = dlopen("libgoprobe.so", RTLD_NOW | RTLD_LOCAL);
    if (handle == NULL) {
        int loader_errno = errno;
        char message[G0_LOADER_ERROR_MAX];
        CaptureLoaderError(message, sizeof(message));
        return BuildResult(env, false, "dlopen", loader_errno, message, 0, 0);
    }

    dlerror();
    void *hello_symbol = dlsym(handle, "Hello");
    void *runtime_symbol = dlsym(handle, "RuntimeProbe");
    if (hello_symbol == NULL || runtime_symbol == NULL) {
        char message[G0_LOADER_ERROR_MAX];
        CaptureLoaderError(message, sizeof(message));
        return BuildResult(env, false, "dlsym", 0, message, 0, 0);
    }

    int hello_value = ((hello_fn)hello_symbol)();
    if (hello_value != 42) {
        return BuildResult(env, false, "hello", 0, "", hello_value, 0);
    }

    long long runtime_value = ((runtime_fn)runtime_symbol)(1048576);
    if (runtime_value != 1048576) {
        return BuildResult(env, false, "runtime", 0, "", hello_value, runtime_value);
    }

    return BuildResult(env, true, "complete", 0, "", hello_value, runtime_value);
}

static napi_value Init(napi_env env, napi_value exports)
{
    napi_property_descriptor property = {
        "runGoProbe", NULL, RunGoProbe, NULL, NULL, NULL, napi_default, NULL
    };
    if (napi_define_properties(env, exports, 1, &property) != napi_ok) {
        napi_throw_error(env, NULL, "failed to export go probe runner");
        return NULL;
    }
    return exports;
}

static napi_module g_goloader_module = {
    1,
    0,
    NULL,
    Init,
    "goloader",
    NULL,
    { 0 },
};

__attribute__((constructor, visibility("default")))
void RegisterGoLoaderModule(void)
{
    napi_module_register(&g_goloader_module);
}
