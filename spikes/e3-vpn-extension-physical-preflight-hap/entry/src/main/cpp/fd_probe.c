#include <errno.h>
#include <fcntl.h>
#include <stdbool.h>

#include <napi/native_api.h>

static napi_value MakeStatus(napi_env env, bool is_open, int error_number)
{
    napi_value result = NULL;
    napi_value open_value = NULL;
    napi_value errno_value = NULL;
    if (napi_create_object(env, &result) != napi_ok ||
        napi_get_boolean(env, is_open, &open_value) != napi_ok ||
        napi_create_int32(env, error_number, &errno_value) != napi_ok ||
        napi_set_named_property(env, result, "open", open_value) != napi_ok ||
        napi_set_named_property(env, result, "errno", errno_value) != napi_ok) {
        napi_throw_error(env, NULL, "failed to build fd status");
        return NULL;
    }
    return result;
}

static napi_value Status(napi_env env, napi_callback_info info)
{
    size_t argc = 1;
    napi_value argv[1] = { NULL };
    int32_t fd = -1;
    if (napi_get_cb_info(env, info, &argc, argv, NULL, NULL) != napi_ok || argc != 1 ||
        napi_get_value_int32(env, argv[0], &fd) != napi_ok) {
        napi_throw_error(env, NULL, "fd must be an integer");
        return NULL;
    }

    errno = 0;
    int result = fcntl(fd, F_GETFD);
    int error_number = result == -1 ? errno : 0;
    return MakeStatus(env, result != -1, error_number);
}

static napi_value Init(napi_env env, napi_value exports)
{
    napi_property_descriptor property = {
        "status", NULL, Status, NULL, NULL, NULL, napi_default, NULL
    };
    if (napi_define_properties(env, exports, 1, &property) != napi_ok) {
        napi_throw_error(env, NULL, "failed to export fd status");
        return NULL;
    }
    return exports;
}

static napi_module g_fdprobe_module = {
    1,
    0,
    NULL,
    Init,
    "fdprobe",
    NULL,
    { 0 },
};

__attribute__((constructor, visibility("default")))
void RegisterFdProbeModule(void)
{
    napi_module_register(&g_fdprobe_module);
}
