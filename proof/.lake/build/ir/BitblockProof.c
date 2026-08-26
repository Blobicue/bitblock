// Lean compiler output
// Module: BitblockProof
// Imports: public import Init public meta import Init public import BitblockProof.World public import BitblockProof.MintBurn public import BitblockProof.QuotedByteCost
#include <lean/lean.h>
#if defined(__clang__)
#pragma clang diagnostic ignored "-Wunused-parameter"
#pragma clang diagnostic ignored "-Wunused-label"
#elif defined(__GNUC__) && !defined(__CLANG__)
#pragma GCC diagnostic ignored "-Wunused-parameter"
#pragma GCC diagnostic ignored "-Wunused-label"
#pragma GCC diagnostic ignored "-Wunused-but-set-variable"
#endif
#ifdef __cplusplus
extern "C" {
#endif
lean_object* initialize_Init(uint8_t builtin);
lean_object* initialize_Init(uint8_t builtin);
lean_object* initialize_bitblock_x2dproof_BitblockProof_World(uint8_t builtin);
lean_object* initialize_bitblock_x2dproof_BitblockProof_MintBurn(uint8_t builtin);
lean_object* initialize_bitblock_x2dproof_BitblockProof_QuotedByteCost(uint8_t builtin);
static bool _G_initialized = false;
LEAN_EXPORT lean_object* initialize_bitblock_x2dproof_BitblockProof(uint8_t builtin) {
lean_object * res;
if (_G_initialized) return lean_io_result_mk_ok(lean_box(0));
_G_initialized = true;
res = initialize_Init(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
res = initialize_Init(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
res = initialize_bitblock_x2dproof_BitblockProof_World(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
res = initialize_bitblock_x2dproof_BitblockProof_MintBurn(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
res = initialize_bitblock_x2dproof_BitblockProof_QuotedByteCost(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
return lean_io_result_mk_ok(lean_box(0));
}
#ifdef __cplusplus
}
#endif
