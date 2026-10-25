#include <lv2/core/lv2.h>
#include <stdlib.h>

typedef struct { float* runs; unsigned count; } Probe;
static LV2_Handle instantiate(const LV2_Descriptor* d, double rate,
                             const char* path, const LV2_Feature* const* features) {
    (void)d; (void)rate; (void)path; (void)features;
    return calloc(1, sizeof(Probe));
}
static void connect_port(LV2_Handle handle, uint32_t port, void* data) {
    if (port == 2) ((Probe*)handle)->runs = data;
}
static void run(LV2_Handle handle, uint32_t frames) {
    (void)frames;
    Probe* probe = handle;
    ++probe->count;
    if (probe->runs) *probe->runs = (float)probe->count;
}
static const LV2_Descriptor descriptor = {
    "urn:livi:test", instantiate, connect_port, NULL, run, NULL, free, NULL
};
LV2_SYMBOL_EXPORT const LV2_Descriptor* lv2_descriptor(uint32_t index) {
    return index == 0 ? &descriptor : NULL;
}
