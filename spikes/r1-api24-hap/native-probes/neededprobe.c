extern int Hello(void);
extern long long RuntimeProbe(long long alloc_bytes);

__attribute__((visibility("default"))) long long CallNeededProbeExports(long long alloc_bytes)
{
    return (long long)Hello() + RuntimeProbe(alloc_bytes);
}
