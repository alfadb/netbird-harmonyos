__thread int tls_probe_value __attribute__((visibility("default"))) = 42;

__attribute__((visibility("default"))) int GetTLS(void)
{
    return tls_probe_value;
}

__attribute__((visibility("default"))) void SetTLS(int value)
{
    tls_probe_value = value;
}

__attribute__((visibility("default"))) void ResetTLS(void)
{
    tls_probe_value = 42;
}
