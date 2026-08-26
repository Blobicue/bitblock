// Lean compiler output
// Module: BitblockProof.World
// Imports: public import Init public meta import Init public import Std
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
uint8_t lean_int_dec_eq(lean_object*, lean_object*);
lean_object* lean_nat_to_int(lean_object*);
lean_object* lean_string_length(lean_object*);
uint8_t lean_int_dec_lt(lean_object*, lean_object*);
lean_object* l_Int_repr(lean_object*);
lean_object* l_Repr_addAppParen(lean_object*, lean_object*);
lean_object* l_instBEqOfDecidableEq___redArg___lam__0___boxed(lean_object*, lean_object*, lean_object*);
uint8_t l_List_elem___redArg(lean_object*, lean_object*, lean_object*);
uint8_t lean_nat_dec_le(lean_object*, lean_object*);
lean_object* l_List_reverse___redArg(lean_object*);
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq___boxed(lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord___boxed(lean_object*, lean_object*);
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 3, .m_capacity = 3, .m_length = 2, .m_data = "{ "};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__0 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__0_value;
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__1_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 2, .m_capacity = 2, .m_length = 1, .m_data = "x"};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__1 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__1_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__2_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__1_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__2 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__2_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__3_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)(((size_t)(0) << 1) | 1)),((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__2_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__3 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__3_value;
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__4_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 5, .m_capacity = 5, .m_length = 4, .m_data = " := "};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__4 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__4_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__5_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__4_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__5 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__5_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__6_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__3_value),((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__5_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__6 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__6_value;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__7_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__7;
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__8_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 3, .m_capacity = 3, .m_length = 2, .m_data = " }"};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__8 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__8_value;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__9_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__9;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__10_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__10;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__11_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__0_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__11 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__11_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__12_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__8_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__12 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__12_value;
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__13_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 2, .m_capacity = 2, .m_length = 1, .m_data = "z"};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__13 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__13_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__14_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__13_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__14 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__14_value;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15;
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__16_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 2, .m_capacity = 2, .m_length = 1, .m_data = ","};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__16 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__16_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__17_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__16_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__17 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__17_value;
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__18_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 2, .m_capacity = 2, .m_length = 1, .m_data = "y"};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__18 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__18_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__19_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__18_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__19 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__19_value;
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg(lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___boxed(lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___boxed(lean_object*, lean_object*);
static const lean_closure_object lp_bitblock_x2dproof_BitblockProof_instReprCoord___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_closure_object) + sizeof(void*)*0, .m_other = 0, .m_tag = 245}, .m_fun = (void*)lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___boxed, .m_arity = 2, .m_num_fixed = 0, .m_objs = {} };
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord___closed__0 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord___closed__0_value;
LEAN_EXPORT const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprCoord___closed__0_value;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_origin___closed__0_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_origin___closed__0;
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_origin;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_initialWorld___closed__0_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_initialWorld___closed__0;
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_initialWorld;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1___closed__0_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1___closed__0;
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1___boxed(lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableBit(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableBit___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_ctorIdx(lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_ctorIdx___boxed(lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___boxed(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_on_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_on_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_off_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_off_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_toggle_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_toggle_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit_decEq(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit_decEq___boxed(lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit___boxed(lean_object*, lean_object*);
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 22, .m_capacity = 22, .m_length = 21, .m_data = "BitblockProof.Edit.on"};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__0 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__0_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__1_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__0_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__1 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__1_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__2_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__1_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__2 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__2_value;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4;
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__5_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "BitblockProof.Edit.off"};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__5 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__5_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__6_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__5_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__6 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__6_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__7_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__6_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__7 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__7_value;
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__8_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "BitblockProof.Edit.toggle"};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__8 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__8_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__9_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__8_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__9 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__9_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__10_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__9_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__10 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__10_value;
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___boxed(lean_object*, lean_object*);
static const lean_closure_object lp_bitblock_x2dproof_BitblockProof_instReprEdit___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_closure_object) + sizeof(void*)*0, .m_other = 0, .m_tag = 245}, .m_fun = (void*)lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___boxed, .m_arity = 2, .m_num_fixed = 0, .m_objs = {} };
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit___closed__0 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit___closed__0_value;
LEAN_EXPORT const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprEdit___closed__0_value;
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_List_filterTR_loop___at___00BitblockProof_remove_spec__0(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_List_filterTR_loop___at___00BitblockProof_remove_spec__0___boxed(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_remove(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_remove___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_applyEdit(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_replay(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_World_0__BitblockProof_instReprEdit_repr_match__1_splitter___redArg(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_World_0__BitblockProof_instReprEdit_repr_match__1_splitter(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_World_0__BitblockProof_replay_match__1_splitter___redArg(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_World_0__BitblockProof_replay_match__1_splitter(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(lean_object* v_x_1_, lean_object* v_x_2_){
_start:
{
lean_object* v_x_3_; lean_object* v_y_4_; lean_object* v_z_5_; lean_object* v_x_6_; lean_object* v_y_7_; lean_object* v_z_8_; uint8_t v___x_9_; 
v_x_3_ = lean_ctor_get(v_x_1_, 0);
v_y_4_ = lean_ctor_get(v_x_1_, 1);
v_z_5_ = lean_ctor_get(v_x_1_, 2);
v_x_6_ = lean_ctor_get(v_x_2_, 0);
v_y_7_ = lean_ctor_get(v_x_2_, 1);
v_z_8_ = lean_ctor_get(v_x_2_, 2);
v___x_9_ = lean_int_dec_eq(v_x_3_, v_x_6_);
if (v___x_9_ == 0)
{
return v___x_9_;
}
else
{
uint8_t v___x_10_; 
v___x_10_ = lean_int_dec_eq(v_y_4_, v_y_7_);
if (v___x_10_ == 0)
{
return v___x_10_;
}
else
{
uint8_t v___x_11_; 
v___x_11_ = lean_int_dec_eq(v_z_5_, v_z_8_);
return v___x_11_;
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq___boxed(lean_object* v_x_12_, lean_object* v_x_13_){
_start:
{
uint8_t v_res_14_; lean_object* v_r_15_; 
v_res_14_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_x_12_, v_x_13_);
lean_dec_ref(v_x_13_);
lean_dec_ref(v_x_12_);
v_r_15_ = lean_box(v_res_14_);
return v_r_15_;
}
}
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord(lean_object* v_x_16_, lean_object* v_x_17_){
_start:
{
uint8_t v___x_18_; 
v___x_18_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_x_16_, v_x_17_);
return v___x_18_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord___boxed(lean_object* v_x_19_, lean_object* v_x_20_){
_start:
{
uint8_t v_res_21_; lean_object* v_r_22_; 
v_res_21_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord(v_x_19_, v_x_20_);
lean_dec_ref(v_x_20_);
lean_dec_ref(v_x_19_);
v_r_22_ = lean_box(v_res_21_);
return v_r_22_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__7(void){
_start:
{
lean_object* v___x_36_; lean_object* v___x_37_; 
v___x_36_ = lean_unsigned_to_nat(5u);
v___x_37_ = lean_nat_to_int(v___x_36_);
return v___x_37_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__9(void){
_start:
{
lean_object* v___x_39_; lean_object* v___x_40_; 
v___x_39_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__0));
v___x_40_ = lean_string_length(v___x_39_);
return v___x_40_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__10(void){
_start:
{
lean_object* v___x_41_; lean_object* v___x_42_; 
v___x_41_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__9, &lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__9_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__9);
v___x_42_ = lean_nat_to_int(v___x_41_);
return v___x_42_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15(void){
_start:
{
lean_object* v___x_50_; lean_object* v___x_51_; 
v___x_50_ = lean_unsigned_to_nat(0u);
v___x_51_ = lean_nat_to_int(v___x_50_);
return v___x_51_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg(lean_object* v_x_58_){
_start:
{
lean_object* v_x_59_; lean_object* v_y_60_; lean_object* v_z_61_; lean_object* v___x_62_; lean_object* v___x_63_; lean_object* v___x_64_; lean_object* v___y_66_; uint8_t v___y_67_; lean_object* v___y_68_; lean_object* v___y_80_; lean_object* v___y_81_; lean_object* v___y_82_; uint8_t v___y_83_; lean_object* v___y_84_; lean_object* v___y_102_; lean_object* v___x_122_; lean_object* v___x_123_; uint8_t v___x_124_; 
v_x_59_ = lean_ctor_get(v_x_58_, 0);
v_y_60_ = lean_ctor_get(v_x_58_, 1);
v_z_61_ = lean_ctor_get(v_x_58_, 2);
v___x_62_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__5));
v___x_63_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__6));
v___x_64_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__7, &lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__7_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__7);
v___x_122_ = lean_unsigned_to_nat(0u);
v___x_123_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15, &lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15);
v___x_124_ = lean_int_dec_lt(v_x_59_, v___x_123_);
if (v___x_124_ == 0)
{
lean_object* v___x_125_; lean_object* v___x_126_; 
v___x_125_ = l_Int_repr(v_x_59_);
v___x_126_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_126_, 0, v___x_125_);
v___y_102_ = v___x_126_;
goto v___jp_101_;
}
else
{
lean_object* v___x_127_; lean_object* v___x_128_; lean_object* v___x_129_; 
v___x_127_ = l_Int_repr(v_x_59_);
v___x_128_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_128_, 0, v___x_127_);
v___x_129_ = l_Repr_addAppParen(v___x_128_, v___x_122_);
v___y_102_ = v___x_129_;
goto v___jp_101_;
}
v___jp_65_:
{
lean_object* v___x_69_; lean_object* v___x_70_; lean_object* v___x_71_; lean_object* v___x_72_; lean_object* v___x_73_; lean_object* v___x_74_; lean_object* v___x_75_; lean_object* v___x_76_; lean_object* v___x_77_; lean_object* v___x_78_; 
v___x_69_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_69_, 0, v___x_64_);
lean_ctor_set(v___x_69_, 1, v___y_68_);
v___x_70_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_70_, 0, v___x_69_);
lean_ctor_set_uint8(v___x_70_, sizeof(void*)*1, v___y_67_);
v___x_71_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_71_, 0, v___y_66_);
lean_ctor_set(v___x_71_, 1, v___x_70_);
v___x_72_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__10, &lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__10_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__10);
v___x_73_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__11));
v___x_74_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_74_, 0, v___x_73_);
lean_ctor_set(v___x_74_, 1, v___x_71_);
v___x_75_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__12));
v___x_76_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_76_, 0, v___x_74_);
lean_ctor_set(v___x_76_, 1, v___x_75_);
v___x_77_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_77_, 0, v___x_72_);
lean_ctor_set(v___x_77_, 1, v___x_76_);
v___x_78_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_78_, 0, v___x_77_);
lean_ctor_set_uint8(v___x_78_, sizeof(void*)*1, v___y_67_);
return v___x_78_;
}
v___jp_79_:
{
lean_object* v___x_85_; lean_object* v___x_86_; lean_object* v___x_87_; lean_object* v___x_88_; lean_object* v___x_89_; lean_object* v___x_90_; lean_object* v___x_91_; lean_object* v___x_92_; lean_object* v___x_93_; lean_object* v___x_94_; uint8_t v___x_95_; 
v___x_85_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_85_, 0, v___x_64_);
lean_ctor_set(v___x_85_, 1, v___y_84_);
v___x_86_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_86_, 0, v___x_85_);
lean_ctor_set_uint8(v___x_86_, sizeof(void*)*1, v___y_83_);
v___x_87_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_87_, 0, v___y_80_);
lean_ctor_set(v___x_87_, 1, v___x_86_);
lean_inc(v___y_82_);
v___x_88_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_88_, 0, v___x_87_);
lean_ctor_set(v___x_88_, 1, v___y_82_);
lean_inc(v___y_81_);
v___x_89_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_89_, 0, v___x_88_);
lean_ctor_set(v___x_89_, 1, v___y_81_);
v___x_90_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__14));
v___x_91_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_91_, 0, v___x_89_);
lean_ctor_set(v___x_91_, 1, v___x_90_);
v___x_92_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_92_, 0, v___x_91_);
lean_ctor_set(v___x_92_, 1, v___x_62_);
v___x_93_ = lean_unsigned_to_nat(0u);
v___x_94_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15, &lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15);
v___x_95_ = lean_int_dec_lt(v_z_61_, v___x_94_);
if (v___x_95_ == 0)
{
lean_object* v___x_96_; lean_object* v___x_97_; 
v___x_96_ = l_Int_repr(v_z_61_);
v___x_97_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_97_, 0, v___x_96_);
v___y_66_ = v___x_92_;
v___y_67_ = v___y_83_;
v___y_68_ = v___x_97_;
goto v___jp_65_;
}
else
{
lean_object* v___x_98_; lean_object* v___x_99_; lean_object* v___x_100_; 
v___x_98_ = l_Int_repr(v_z_61_);
v___x_99_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_99_, 0, v___x_98_);
v___x_100_ = l_Repr_addAppParen(v___x_99_, v___x_93_);
v___y_66_ = v___x_92_;
v___y_67_ = v___y_83_;
v___y_68_ = v___x_100_;
goto v___jp_65_;
}
}
v___jp_101_:
{
lean_object* v___x_103_; uint8_t v___x_104_; lean_object* v___x_105_; lean_object* v___x_106_; lean_object* v___x_107_; lean_object* v___x_108_; lean_object* v___x_109_; lean_object* v___x_110_; lean_object* v___x_111_; lean_object* v___x_112_; lean_object* v___x_113_; lean_object* v___x_114_; lean_object* v___x_115_; uint8_t v___x_116_; 
v___x_103_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_103_, 0, v___x_64_);
lean_ctor_set(v___x_103_, 1, v___y_102_);
v___x_104_ = 0;
v___x_105_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_105_, 0, v___x_103_);
lean_ctor_set_uint8(v___x_105_, sizeof(void*)*1, v___x_104_);
v___x_106_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_106_, 0, v___x_63_);
lean_ctor_set(v___x_106_, 1, v___x_105_);
v___x_107_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__17));
v___x_108_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_108_, 0, v___x_106_);
lean_ctor_set(v___x_108_, 1, v___x_107_);
v___x_109_ = lean_box(1);
v___x_110_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_110_, 0, v___x_108_);
lean_ctor_set(v___x_110_, 1, v___x_109_);
v___x_111_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__19));
v___x_112_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_112_, 0, v___x_110_);
lean_ctor_set(v___x_112_, 1, v___x_111_);
v___x_113_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_113_, 0, v___x_112_);
lean_ctor_set(v___x_113_, 1, v___x_62_);
v___x_114_ = lean_unsigned_to_nat(0u);
v___x_115_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15, &lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15);
v___x_116_ = lean_int_dec_lt(v_y_60_, v___x_115_);
if (v___x_116_ == 0)
{
lean_object* v___x_117_; lean_object* v___x_118_; 
v___x_117_ = l_Int_repr(v_y_60_);
v___x_118_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_118_, 0, v___x_117_);
v___y_80_ = v___x_113_;
v___y_81_ = v___x_109_;
v___y_82_ = v___x_107_;
v___y_83_ = v___x_104_;
v___y_84_ = v___x_118_;
goto v___jp_79_;
}
else
{
lean_object* v___x_119_; lean_object* v___x_120_; lean_object* v___x_121_; 
v___x_119_ = l_Int_repr(v_y_60_);
v___x_120_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_120_, 0, v___x_119_);
v___x_121_ = l_Repr_addAppParen(v___x_120_, v___x_114_);
v___y_80_ = v___x_113_;
v___y_81_ = v___x_109_;
v___y_82_ = v___x_107_;
v___y_83_ = v___x_104_;
v___y_84_ = v___x_121_;
goto v___jp_79_;
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___boxed(lean_object* v_x_130_){
_start:
{
lean_object* v_res_131_; 
v_res_131_ = lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg(v_x_130_);
lean_dec_ref(v_x_130_);
return v_res_131_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr(lean_object* v_x_132_, lean_object* v_prec_133_){
_start:
{
lean_object* v___x_134_; 
v___x_134_ = lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg(v_x_132_);
return v___x_134_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___boxed(lean_object* v_x_135_, lean_object* v_prec_136_){
_start:
{
lean_object* v_res_137_; 
v_res_137_ = lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr(v_x_135_, v_prec_136_);
lean_dec(v_prec_136_);
lean_dec_ref(v_x_135_);
return v_res_137_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_origin___closed__0(void){
_start:
{
lean_object* v___x_140_; lean_object* v___x_141_; 
v___x_140_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15, &lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg___closed__15);
v___x_141_ = lean_alloc_ctor(0, 3, 0);
lean_ctor_set(v___x_141_, 0, v___x_140_);
lean_ctor_set(v___x_141_, 1, v___x_140_);
lean_ctor_set(v___x_141_, 2, v___x_140_);
return v___x_141_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_origin(void){
_start:
{
lean_object* v___x_142_; 
v___x_142_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_origin___closed__0, &lp_bitblock_x2dproof_BitblockProof_origin___closed__0_once, _init_lp_bitblock_x2dproof_BitblockProof_origin___closed__0);
return v___x_142_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_initialWorld___closed__0(void){
_start:
{
lean_object* v___x_143_; lean_object* v___x_144_; lean_object* v___x_145_; 
v___x_143_ = lean_box(0);
v___x_144_ = lp_bitblock_x2dproof_BitblockProof_origin;
v___x_145_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v___x_145_, 0, v___x_144_);
lean_ctor_set(v___x_145_, 1, v___x_143_);
return v___x_145_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_initialWorld(void){
_start:
{
lean_object* v___x_146_; 
v___x_146_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_initialWorld___closed__0, &lp_bitblock_x2dproof_BitblockProof_initialWorld___closed__0_once, _init_lp_bitblock_x2dproof_BitblockProof_initialWorld___closed__0);
return v___x_146_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1___closed__0(void){
_start:
{
lean_object* v___x_147_; lean_object* v___f_148_; 
v___x_147_ = lean_alloc_closure((void*)(lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord___boxed), 2, 0);
v___f_148_ = lean_alloc_closure((void*)(l_instBEqOfDecidableEq___redArg___lam__0___boxed), 3, 1);
lean_closure_set(v___f_148_, 0, v___x_147_);
return v___f_148_;
}
}
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1(lean_object* v_world_149_, lean_object* v_coord_150_){
_start:
{
lean_object* v___f_151_; uint8_t v___x_152_; 
v___f_151_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1___closed__0, &lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1___closed__0_once, _init_lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1___closed__0);
v___x_152_ = l_List_elem___redArg(v___f_151_, v_coord_150_, v_world_149_);
return v___x_152_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1___boxed(lean_object* v_world_153_, lean_object* v_coord_154_){
_start:
{
uint8_t v_res_155_; lean_object* v_r_156_; 
v_res_155_ = lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1(v_world_153_, v_coord_154_);
v_r_156_ = lean_box(v_res_155_);
return v_r_156_;
}
}
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableBit(lean_object* v_world_157_, lean_object* v_coord_158_){
_start:
{
uint8_t v___x_159_; 
v___x_159_ = lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1(v_world_157_, v_coord_158_);
return v___x_159_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableBit___boxed(lean_object* v_world_160_, lean_object* v_coord_161_){
_start:
{
uint8_t v_res_162_; lean_object* v_r_163_; 
v_res_162_ = lp_bitblock_x2dproof_BitblockProof_instDecidableBit(v_world_160_, v_coord_161_);
v_r_163_ = lean_box(v_res_162_);
return v_r_163_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_ctorIdx(lean_object* v_x_164_){
_start:
{
switch(lean_obj_tag(v_x_164_))
{
case 0:
{
lean_object* v___x_165_; 
v___x_165_ = lean_unsigned_to_nat(0u);
return v___x_165_;
}
case 1:
{
lean_object* v___x_166_; 
v___x_166_ = lean_unsigned_to_nat(1u);
return v___x_166_;
}
default: 
{
lean_object* v___x_167_; 
v___x_167_ = lean_unsigned_to_nat(2u);
return v___x_167_;
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_ctorIdx___boxed(lean_object* v_x_168_){
_start:
{
lean_object* v_res_169_; 
v_res_169_ = lp_bitblock_x2dproof_BitblockProof_Edit_ctorIdx(v_x_168_);
lean_dec_ref(v_x_168_);
return v_res_169_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___redArg(lean_object* v_t_170_, lean_object* v_k_171_){
_start:
{
lean_object* v_coord_172_; lean_object* v___x_173_; 
v_coord_172_ = lean_ctor_get(v_t_170_, 0);
lean_inc_ref(v_coord_172_);
lean_dec_ref(v_t_170_);
v___x_173_ = lean_apply_1(v_k_171_, v_coord_172_);
return v___x_173_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim(lean_object* v_motive_174_, lean_object* v_ctorIdx_175_, lean_object* v_t_176_, lean_object* v_h_177_, lean_object* v_k_178_){
_start:
{
lean_object* v___x_179_; 
v___x_179_ = lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___redArg(v_t_176_, v_k_178_);
return v___x_179_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___boxed(lean_object* v_motive_180_, lean_object* v_ctorIdx_181_, lean_object* v_t_182_, lean_object* v_h_183_, lean_object* v_k_184_){
_start:
{
lean_object* v_res_185_; 
v_res_185_ = lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim(v_motive_180_, v_ctorIdx_181_, v_t_182_, v_h_183_, v_k_184_);
lean_dec(v_ctorIdx_181_);
return v_res_185_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_on_elim___redArg(lean_object* v_t_186_, lean_object* v_on_187_){
_start:
{
lean_object* v___x_188_; 
v___x_188_ = lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___redArg(v_t_186_, v_on_187_);
return v___x_188_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_on_elim(lean_object* v_motive_189_, lean_object* v_t_190_, lean_object* v_h_191_, lean_object* v_on_192_){
_start:
{
lean_object* v___x_193_; 
v___x_193_ = lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___redArg(v_t_190_, v_on_192_);
return v___x_193_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_off_elim___redArg(lean_object* v_t_194_, lean_object* v_off_195_){
_start:
{
lean_object* v___x_196_; 
v___x_196_ = lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___redArg(v_t_194_, v_off_195_);
return v___x_196_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_off_elim(lean_object* v_motive_197_, lean_object* v_t_198_, lean_object* v_h_199_, lean_object* v_off_200_){
_start:
{
lean_object* v___x_201_; 
v___x_201_ = lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___redArg(v_t_198_, v_off_200_);
return v___x_201_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_toggle_elim___redArg(lean_object* v_t_202_, lean_object* v_toggle_203_){
_start:
{
lean_object* v___x_204_; 
v___x_204_ = lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___redArg(v_t_202_, v_toggle_203_);
return v___x_204_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_Edit_toggle_elim(lean_object* v_motive_205_, lean_object* v_t_206_, lean_object* v_h_207_, lean_object* v_toggle_208_){
_start:
{
lean_object* v___x_209_; 
v___x_209_ = lp_bitblock_x2dproof_BitblockProof_Edit_ctorElim___redArg(v_t_206_, v_toggle_208_);
return v___x_209_;
}
}
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit_decEq(lean_object* v_x_210_, lean_object* v_x_211_){
_start:
{
switch(lean_obj_tag(v_x_210_))
{
case 0:
{
if (lean_obj_tag(v_x_211_) == 0)
{
lean_object* v_coord_212_; lean_object* v_coord_213_; uint8_t v___x_214_; 
v_coord_212_ = lean_ctor_get(v_x_210_, 0);
v_coord_213_ = lean_ctor_get(v_x_211_, 0);
v___x_214_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_coord_212_, v_coord_213_);
return v___x_214_;
}
else
{
uint8_t v___x_215_; 
v___x_215_ = 0;
return v___x_215_;
}
}
case 1:
{
if (lean_obj_tag(v_x_211_) == 1)
{
lean_object* v_coord_216_; lean_object* v_coord_217_; uint8_t v___x_218_; 
v_coord_216_ = lean_ctor_get(v_x_210_, 0);
v_coord_217_ = lean_ctor_get(v_x_211_, 0);
v___x_218_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_coord_216_, v_coord_217_);
return v___x_218_;
}
else
{
uint8_t v___x_219_; 
v___x_219_ = 0;
return v___x_219_;
}
}
default: 
{
if (lean_obj_tag(v_x_211_) == 2)
{
lean_object* v_coord_220_; lean_object* v_coord_221_; uint8_t v___x_222_; 
v_coord_220_ = lean_ctor_get(v_x_210_, 0);
v_coord_221_ = lean_ctor_get(v_x_211_, 0);
v___x_222_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_coord_220_, v_coord_221_);
return v___x_222_;
}
else
{
uint8_t v___x_223_; 
v___x_223_ = 0;
return v___x_223_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit_decEq___boxed(lean_object* v_x_224_, lean_object* v_x_225_){
_start:
{
uint8_t v_res_226_; lean_object* v_r_227_; 
v_res_226_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit_decEq(v_x_224_, v_x_225_);
lean_dec_ref(v_x_225_);
lean_dec_ref(v_x_224_);
v_r_227_ = lean_box(v_res_226_);
return v_r_227_;
}
}
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit(lean_object* v_x_228_, lean_object* v_x_229_){
_start:
{
uint8_t v___x_230_; 
v___x_230_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit_decEq(v_x_228_, v_x_229_);
return v___x_230_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit___boxed(lean_object* v_x_231_, lean_object* v_x_232_){
_start:
{
uint8_t v_res_233_; lean_object* v_r_234_; 
v_res_233_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqEdit(v_x_231_, v_x_232_);
lean_dec_ref(v_x_232_);
lean_dec_ref(v_x_231_);
v_r_234_ = lean_box(v_res_233_);
return v_r_234_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3(void){
_start:
{
lean_object* v___x_241_; lean_object* v___x_242_; 
v___x_241_ = lean_unsigned_to_nat(2u);
v___x_242_ = lean_nat_to_int(v___x_241_);
return v___x_242_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4(void){
_start:
{
lean_object* v___x_243_; lean_object* v___x_244_; 
v___x_243_ = lean_unsigned_to_nat(1u);
v___x_244_ = lean_nat_to_int(v___x_243_);
return v___x_244_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr(lean_object* v_x_257_, lean_object* v_prec_258_){
_start:
{
switch(lean_obj_tag(v_x_257_))
{
case 0:
{
lean_object* v_coord_259_; lean_object* v___y_261_; lean_object* v___x_269_; uint8_t v___x_270_; 
v_coord_259_ = lean_ctor_get(v_x_257_, 0);
v___x_269_ = lean_unsigned_to_nat(1024u);
v___x_270_ = lean_nat_dec_le(v___x_269_, v_prec_258_);
if (v___x_270_ == 0)
{
lean_object* v___x_271_; 
v___x_271_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3, &lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3);
v___y_261_ = v___x_271_;
goto v___jp_260_;
}
else
{
lean_object* v___x_272_; 
v___x_272_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4, &lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4);
v___y_261_ = v___x_272_;
goto v___jp_260_;
}
v___jp_260_:
{
lean_object* v___x_262_; lean_object* v___x_263_; lean_object* v___x_264_; lean_object* v___x_265_; uint8_t v___x_266_; lean_object* v___x_267_; lean_object* v___x_268_; 
v___x_262_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__2));
v___x_263_ = lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg(v_coord_259_);
v___x_264_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_264_, 0, v___x_262_);
lean_ctor_set(v___x_264_, 1, v___x_263_);
lean_inc(v___y_261_);
v___x_265_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_265_, 0, v___y_261_);
lean_ctor_set(v___x_265_, 1, v___x_264_);
v___x_266_ = 0;
v___x_267_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_267_, 0, v___x_265_);
lean_ctor_set_uint8(v___x_267_, sizeof(void*)*1, v___x_266_);
v___x_268_ = l_Repr_addAppParen(v___x_267_, v_prec_258_);
return v___x_268_;
}
}
case 1:
{
lean_object* v_coord_273_; lean_object* v___y_275_; lean_object* v___x_283_; uint8_t v___x_284_; 
v_coord_273_ = lean_ctor_get(v_x_257_, 0);
v___x_283_ = lean_unsigned_to_nat(1024u);
v___x_284_ = lean_nat_dec_le(v___x_283_, v_prec_258_);
if (v___x_284_ == 0)
{
lean_object* v___x_285_; 
v___x_285_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3, &lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3);
v___y_275_ = v___x_285_;
goto v___jp_274_;
}
else
{
lean_object* v___x_286_; 
v___x_286_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4, &lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4);
v___y_275_ = v___x_286_;
goto v___jp_274_;
}
v___jp_274_:
{
lean_object* v___x_276_; lean_object* v___x_277_; lean_object* v___x_278_; lean_object* v___x_279_; uint8_t v___x_280_; lean_object* v___x_281_; lean_object* v___x_282_; 
v___x_276_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__7));
v___x_277_ = lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg(v_coord_273_);
v___x_278_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_278_, 0, v___x_276_);
lean_ctor_set(v___x_278_, 1, v___x_277_);
lean_inc(v___y_275_);
v___x_279_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_279_, 0, v___y_275_);
lean_ctor_set(v___x_279_, 1, v___x_278_);
v___x_280_ = 0;
v___x_281_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_281_, 0, v___x_279_);
lean_ctor_set_uint8(v___x_281_, sizeof(void*)*1, v___x_280_);
v___x_282_ = l_Repr_addAppParen(v___x_281_, v_prec_258_);
return v___x_282_;
}
}
default: 
{
lean_object* v_coord_287_; lean_object* v___y_289_; lean_object* v___x_297_; uint8_t v___x_298_; 
v_coord_287_ = lean_ctor_get(v_x_257_, 0);
v___x_297_ = lean_unsigned_to_nat(1024u);
v___x_298_ = lean_nat_dec_le(v___x_297_, v_prec_258_);
if (v___x_298_ == 0)
{
lean_object* v___x_299_; 
v___x_299_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3, &lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__3);
v___y_289_ = v___x_299_;
goto v___jp_288_;
}
else
{
lean_object* v___x_300_; 
v___x_300_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4, &lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__4);
v___y_289_ = v___x_300_;
goto v___jp_288_;
}
v___jp_288_:
{
lean_object* v___x_290_; lean_object* v___x_291_; lean_object* v___x_292_; lean_object* v___x_293_; uint8_t v___x_294_; lean_object* v___x_295_; lean_object* v___x_296_; 
v___x_290_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___closed__10));
v___x_291_ = lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg(v_coord_287_);
v___x_292_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_292_, 0, v___x_290_);
lean_ctor_set(v___x_292_, 1, v___x_291_);
lean_inc(v___y_289_);
v___x_293_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_293_, 0, v___y_289_);
lean_ctor_set(v___x_293_, 1, v___x_292_);
v___x_294_ = 0;
v___x_295_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_295_, 0, v___x_293_);
lean_ctor_set_uint8(v___x_295_, sizeof(void*)*1, v___x_294_);
v___x_296_ = l_Repr_addAppParen(v___x_295_, v_prec_258_);
return v___x_296_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr___boxed(lean_object* v_x_301_, lean_object* v_prec_302_){
_start:
{
lean_object* v_res_303_; 
v_res_303_ = lp_bitblock_x2dproof_BitblockProof_instReprEdit_repr(v_x_301_, v_prec_302_);
lean_dec(v_prec_302_);
lean_dec_ref(v_x_301_);
return v_res_303_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_List_filterTR_loop___at___00BitblockProof_remove_spec__0(lean_object* v_coord_306_, lean_object* v_a_307_, lean_object* v_a_308_){
_start:
{
if (lean_obj_tag(v_a_307_) == 0)
{
lean_object* v___x_309_; 
v___x_309_ = l_List_reverse___redArg(v_a_308_);
return v___x_309_;
}
else
{
lean_object* v_head_310_; lean_object* v_tail_311_; lean_object* v___x_313_; uint8_t v_isShared_314_; uint8_t v_isSharedCheck_321_; 
v_head_310_ = lean_ctor_get(v_a_307_, 0);
v_tail_311_ = lean_ctor_get(v_a_307_, 1);
v_isSharedCheck_321_ = !lean_is_exclusive(v_a_307_);
if (v_isSharedCheck_321_ == 0)
{
v___x_313_ = v_a_307_;
v_isShared_314_ = v_isSharedCheck_321_;
goto v_resetjp_312_;
}
else
{
lean_inc(v_tail_311_);
lean_inc(v_head_310_);
lean_dec(v_a_307_);
v___x_313_ = lean_box(0);
v_isShared_314_ = v_isSharedCheck_321_;
goto v_resetjp_312_;
}
v_resetjp_312_:
{
uint8_t v___x_315_; 
v___x_315_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_head_310_, v_coord_306_);
if (v___x_315_ == 0)
{
lean_object* v___x_317_; 
if (v_isShared_314_ == 0)
{
lean_ctor_set(v___x_313_, 1, v_a_308_);
v___x_317_ = v___x_313_;
goto v_reusejp_316_;
}
else
{
lean_object* v_reuseFailAlloc_319_; 
v_reuseFailAlloc_319_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v_reuseFailAlloc_319_, 0, v_head_310_);
lean_ctor_set(v_reuseFailAlloc_319_, 1, v_a_308_);
v___x_317_ = v_reuseFailAlloc_319_;
goto v_reusejp_316_;
}
v_reusejp_316_:
{
v_a_307_ = v_tail_311_;
v_a_308_ = v___x_317_;
goto _start;
}
}
else
{
lean_del_object(v___x_313_);
lean_dec(v_head_310_);
v_a_307_ = v_tail_311_;
goto _start;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_List_filterTR_loop___at___00BitblockProof_remove_spec__0___boxed(lean_object* v_coord_322_, lean_object* v_a_323_, lean_object* v_a_324_){
_start:
{
lean_object* v_res_325_; 
v_res_325_ = lp_bitblock_x2dproof_List_filterTR_loop___at___00BitblockProof_remove_spec__0(v_coord_322_, v_a_323_, v_a_324_);
lean_dec_ref(v_coord_322_);
return v_res_325_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_remove(lean_object* v_world_326_, lean_object* v_coord_327_){
_start:
{
lean_object* v___x_328_; lean_object* v___x_329_; 
v___x_328_ = lean_box(0);
v___x_329_ = lp_bitblock_x2dproof_List_filterTR_loop___at___00BitblockProof_remove_spec__0(v_coord_327_, v_world_326_, v___x_328_);
return v___x_329_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_remove___boxed(lean_object* v_world_330_, lean_object* v_coord_331_){
_start:
{
lean_object* v_res_332_; 
v_res_332_ = lp_bitblock_x2dproof_BitblockProof_remove(v_world_330_, v_coord_331_);
lean_dec_ref(v_coord_331_);
return v_res_332_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_applyEdit(lean_object* v_world_333_, lean_object* v_x_334_){
_start:
{
switch(lean_obj_tag(v_x_334_))
{
case 0:
{
lean_object* v_coord_335_; lean_object* v___x_336_; 
v_coord_335_ = lean_ctor_get(v_x_334_, 0);
lean_inc_ref(v_coord_335_);
lean_dec_ref_known(v_x_334_, 1);
v___x_336_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v___x_336_, 0, v_coord_335_);
lean_ctor_set(v___x_336_, 1, v_world_333_);
return v___x_336_;
}
case 1:
{
lean_object* v_coord_337_; lean_object* v___x_338_; uint8_t v___x_339_; 
v_coord_337_ = lean_ctor_get(v_x_334_, 0);
lean_inc_ref(v_coord_337_);
lean_dec_ref_known(v_x_334_, 1);
v___x_338_ = lp_bitblock_x2dproof_BitblockProof_origin;
v___x_339_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_coord_337_, v___x_338_);
if (v___x_339_ == 0)
{
lean_object* v___x_340_; 
v___x_340_ = lp_bitblock_x2dproof_BitblockProof_remove(v_world_333_, v_coord_337_);
lean_dec_ref(v_coord_337_);
return v___x_340_;
}
else
{
lean_dec_ref(v_coord_337_);
return v_world_333_;
}
}
default: 
{
lean_object* v_coord_341_; lean_object* v___x_342_; uint8_t v___x_343_; 
v_coord_341_ = lean_ctor_get(v_x_334_, 0);
lean_inc_ref(v_coord_341_);
lean_dec_ref_known(v_x_334_, 1);
v___x_342_ = lp_bitblock_x2dproof_BitblockProof_origin;
v___x_343_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_coord_341_, v___x_342_);
if (v___x_343_ == 0)
{
uint8_t v___x_344_; 
lean_inc_ref(v_coord_341_);
lean_inc(v_world_333_);
v___x_344_ = lp_bitblock_x2dproof_BitblockProof_instDecidableBit___aux__1(v_world_333_, v_coord_341_);
if (v___x_344_ == 0)
{
lean_object* v___x_345_; 
v___x_345_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v___x_345_, 0, v_coord_341_);
lean_ctor_set(v___x_345_, 1, v_world_333_);
return v___x_345_;
}
else
{
lean_object* v___x_346_; 
v___x_346_ = lp_bitblock_x2dproof_BitblockProof_remove(v_world_333_, v_coord_341_);
lean_dec_ref(v_coord_341_);
return v___x_346_;
}
}
else
{
lean_dec_ref(v_coord_341_);
return v_world_333_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_replay(lean_object* v_x_347_, lean_object* v_x_348_){
_start:
{
if (lean_obj_tag(v_x_348_) == 0)
{
return v_x_347_;
}
else
{
lean_object* v_head_349_; lean_object* v_tail_350_; lean_object* v___x_351_; 
v_head_349_ = lean_ctor_get(v_x_348_, 0);
lean_inc(v_head_349_);
v_tail_350_ = lean_ctor_get(v_x_348_, 1);
lean_inc(v_tail_350_);
lean_dec_ref_known(v_x_348_, 2);
v___x_351_ = lp_bitblock_x2dproof_BitblockProof_applyEdit(v_x_347_, v_head_349_);
v_x_347_ = v___x_351_;
v_x_348_ = v_tail_350_;
goto _start;
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_World_0__BitblockProof_instReprEdit_repr_match__1_splitter___redArg(lean_object* v_x_353_, lean_object* v_h__1_354_, lean_object* v_h__2_355_, lean_object* v_h__3_356_){
_start:
{
switch(lean_obj_tag(v_x_353_))
{
case 0:
{
lean_object* v_coord_357_; lean_object* v___x_358_; 
lean_dec(v_h__3_356_);
lean_dec(v_h__2_355_);
v_coord_357_ = lean_ctor_get(v_x_353_, 0);
lean_inc_ref(v_coord_357_);
lean_dec_ref_known(v_x_353_, 1);
v___x_358_ = lean_apply_1(v_h__1_354_, v_coord_357_);
return v___x_358_;
}
case 1:
{
lean_object* v_coord_359_; lean_object* v___x_360_; 
lean_dec(v_h__3_356_);
lean_dec(v_h__1_354_);
v_coord_359_ = lean_ctor_get(v_x_353_, 0);
lean_inc_ref(v_coord_359_);
lean_dec_ref_known(v_x_353_, 1);
v___x_360_ = lean_apply_1(v_h__2_355_, v_coord_359_);
return v___x_360_;
}
default: 
{
lean_object* v_coord_361_; lean_object* v___x_362_; 
lean_dec(v_h__2_355_);
lean_dec(v_h__1_354_);
v_coord_361_ = lean_ctor_get(v_x_353_, 0);
lean_inc_ref(v_coord_361_);
lean_dec_ref_known(v_x_353_, 1);
v___x_362_ = lean_apply_1(v_h__3_356_, v_coord_361_);
return v___x_362_;
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_World_0__BitblockProof_instReprEdit_repr_match__1_splitter(lean_object* v_motive_363_, lean_object* v_x_364_, lean_object* v_h__1_365_, lean_object* v_h__2_366_, lean_object* v_h__3_367_){
_start:
{
switch(lean_obj_tag(v_x_364_))
{
case 0:
{
lean_object* v_coord_368_; lean_object* v___x_369_; 
lean_dec(v_h__3_367_);
lean_dec(v_h__2_366_);
v_coord_368_ = lean_ctor_get(v_x_364_, 0);
lean_inc_ref(v_coord_368_);
lean_dec_ref_known(v_x_364_, 1);
v___x_369_ = lean_apply_1(v_h__1_365_, v_coord_368_);
return v___x_369_;
}
case 1:
{
lean_object* v_coord_370_; lean_object* v___x_371_; 
lean_dec(v_h__3_367_);
lean_dec(v_h__1_365_);
v_coord_370_ = lean_ctor_get(v_x_364_, 0);
lean_inc_ref(v_coord_370_);
lean_dec_ref_known(v_x_364_, 1);
v___x_371_ = lean_apply_1(v_h__2_366_, v_coord_370_);
return v___x_371_;
}
default: 
{
lean_object* v_coord_372_; lean_object* v___x_373_; 
lean_dec(v_h__2_366_);
lean_dec(v_h__1_365_);
v_coord_372_ = lean_ctor_get(v_x_364_, 0);
lean_inc_ref(v_coord_372_);
lean_dec_ref_known(v_x_364_, 1);
v___x_373_ = lean_apply_1(v_h__3_367_, v_coord_372_);
return v___x_373_;
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_World_0__BitblockProof_replay_match__1_splitter___redArg(lean_object* v_x_374_, lean_object* v_x_375_, lean_object* v_h__1_376_, lean_object* v_h__2_377_){
_start:
{
if (lean_obj_tag(v_x_375_) == 0)
{
lean_object* v___x_378_; 
lean_dec(v_h__2_377_);
v___x_378_ = lean_apply_1(v_h__1_376_, v_x_374_);
return v___x_378_;
}
else
{
lean_object* v_head_379_; lean_object* v_tail_380_; lean_object* v___x_381_; 
lean_dec(v_h__1_376_);
v_head_379_ = lean_ctor_get(v_x_375_, 0);
lean_inc(v_head_379_);
v_tail_380_ = lean_ctor_get(v_x_375_, 1);
lean_inc(v_tail_380_);
lean_dec_ref_known(v_x_375_, 2);
v___x_381_ = lean_apply_3(v_h__2_377_, v_x_374_, v_head_379_, v_tail_380_);
return v___x_381_;
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_World_0__BitblockProof_replay_match__1_splitter(lean_object* v_motive_382_, lean_object* v_x_383_, lean_object* v_x_384_, lean_object* v_h__1_385_, lean_object* v_h__2_386_){
_start:
{
if (lean_obj_tag(v_x_384_) == 0)
{
lean_object* v___x_387_; 
lean_dec(v_h__2_386_);
v___x_387_ = lean_apply_1(v_h__1_385_, v_x_383_);
return v___x_387_;
}
else
{
lean_object* v_head_388_; lean_object* v_tail_389_; lean_object* v___x_390_; 
lean_dec(v_h__1_385_);
v_head_388_ = lean_ctor_get(v_x_384_, 0);
lean_inc(v_head_388_);
v_tail_389_ = lean_ctor_get(v_x_384_, 1);
lean_inc(v_tail_389_);
lean_dec_ref_known(v_x_384_, 2);
v___x_390_ = lean_apply_3(v_h__2_386_, v_x_383_, v_head_388_, v_tail_389_);
return v___x_390_;
}
}
}
lean_object* initialize_Init(uint8_t builtin);
lean_object* initialize_Init(uint8_t builtin);
lean_object* initialize_Std(uint8_t builtin);
static bool _G_initialized = false;
LEAN_EXPORT lean_object* initialize_bitblock_x2dproof_BitblockProof_World(uint8_t builtin) {
lean_object * res;
if (_G_initialized) return lean_io_result_mk_ok(lean_box(0));
_G_initialized = true;
res = initialize_Init(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
res = initialize_Init(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
res = initialize_Std(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
lp_bitblock_x2dproof_BitblockProof_origin = _init_lp_bitblock_x2dproof_BitblockProof_origin();
lean_mark_persistent(lp_bitblock_x2dproof_BitblockProof_origin);
lp_bitblock_x2dproof_BitblockProof_initialWorld = _init_lp_bitblock_x2dproof_BitblockProof_initialWorld();
lean_mark_persistent(lp_bitblock_x2dproof_BitblockProof_initialWorld);
return lean_io_result_mk_ok(lean_box(0));
}
#ifdef __cplusplus
}
#endif
