// Lean compiler output
// Module: BitblockProof.MintBurn
// Imports: public import Init public meta import Init public import BitblockProof.World
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
extern lean_object* lp_bitblock_x2dproof_BitblockProof_origin;
lean_object* lean_mk_empty_array_with_capacity(lean_object*);
uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(lean_object*, lean_object*);
lean_object* lean_array_push(lean_object*, lean_object*);
lean_object* lean_array_get_size(lean_object*);
uint8_t lean_nat_dec_lt(lean_object*, lean_object*);
size_t lean_usize_of_nat(lean_object*);
uint8_t lean_usize_dec_eq(size_t, size_t);
size_t lean_usize_sub(size_t, size_t);
lean_object* lean_array_uget_borrowed(lean_object*, size_t);
lean_object* l_List_lengthTR___redArg(lean_object*);
lean_object* lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg(lean_object*);
lean_object* l_Repr_addAppParen(lean_object*, lean_object*);
uint8_t lean_nat_dec_le(lean_object*, lean_object*);
lean_object* lean_nat_to_int(lean_object*);
lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord___boxed(lean_object*, lean_object*);
lean_object* l_instBEqOfDecidableEq___redArg___lam__0___boxed(lean_object*, lean_object*, lean_object*);
uint8_t l_List_elem___redArg(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_Init_Data_Array_Basic_0__Array_foldrMUnsafe_fold___at___00__private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0_spec__0(lean_object*, size_t, size_t, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_Init_Data_Array_Basic_0__Array_foldrMUnsafe_fold___at___00__private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0_spec__0___boxed(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0___boxed(lean_object*, lean_object*, lean_object*, lean_object*);
static const lean_array_object lp_bitblock_x2dproof_BitblockProof_mutableSupply___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_array_object) + sizeof(void*)*0, .m_other = 0, .m_tag = 246}, .m_size = 0, .m_capacity = 0, .m_data = {}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_mutableSupply___closed__0 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_mutableSupply___closed__0_value;
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_mutableSupply(lean_object*);
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_mint___closed__0_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_mint___closed__0;
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_mint(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_burn(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorIdx(lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorIdx___boxed(lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim___boxed(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_mint_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_mint_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_burn_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_burn_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit_decEq(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit_decEq___boxed(lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit___boxed(lean_object*, lean_object*);
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 30, .m_capacity = 30, .m_length = 29, .m_data = "BitblockProof.SupplyEdit.mint"};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__0 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__0_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__1_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__0_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__1 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__1_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__2_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__1_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__2 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__2_value;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__3_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__3;
static lean_once_cell_t lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__4_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__4;
static const lean_string_object lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__5_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 30, .m_capacity = 30, .m_length = 29, .m_data = "BitblockProof.SupplyEdit.burn"};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__5 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__5_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__6_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__5_value)}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__6 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__6_value;
static const lean_ctor_object lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__7_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__6_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__7 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__7_value;
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___boxed(lean_object*, lean_object*);
static const lean_closure_object lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_closure_object) + sizeof(void*)*0, .m_other = 0, .m_tag = 245}, .m_fun = (void*)lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___boxed, .m_arity = 2, .m_num_fixed = 0, .m_objs = {} };
static const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit___closed__0 = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit___closed__0_value;
LEAN_EXPORT const lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit = (const lean_object*)&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit___closed__0_value;
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_applySupplyEdit(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_replaySupply(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_MintBurn_0__BitblockProof_instReprSupplyEdit_repr_match__1_splitter___redArg(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_MintBurn_0__BitblockProof_instReprSupplyEdit_repr_match__1_splitter(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_Init_Data_Array_Basic_0__Array_foldrMUnsafe_fold___at___00__private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0_spec__0(lean_object* v_as_1_, size_t v_i_2_, size_t v_stop_3_, lean_object* v_b_4_){
_start:
{
uint8_t v___x_5_; 
v___x_5_ = lean_usize_dec_eq(v_i_2_, v_stop_3_);
if (v___x_5_ == 0)
{
size_t v___x_6_; size_t v___x_7_; lean_object* v___x_8_; lean_object* v___x_9_; 
v___x_6_ = ((size_t)1ULL);
v___x_7_ = lean_usize_sub(v_i_2_, v___x_6_);
v___x_8_ = lean_array_uget_borrowed(v_as_1_, v___x_7_);
lean_inc(v___x_8_);
v___x_9_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v___x_9_, 0, v___x_8_);
lean_ctor_set(v___x_9_, 1, v_b_4_);
v_i_2_ = v___x_7_;
v_b_4_ = v___x_9_;
goto _start;
}
else
{
return v_b_4_;
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_Init_Data_Array_Basic_0__Array_foldrMUnsafe_fold___at___00__private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0_spec__0___boxed(lean_object* v_as_11_, lean_object* v_i_12_, lean_object* v_stop_13_, lean_object* v_b_14_){
_start:
{
size_t v_i_boxed_15_; size_t v_stop_boxed_16_; lean_object* v_res_17_; 
v_i_boxed_15_ = lean_unbox_usize(v_i_12_);
lean_dec(v_i_12_);
v_stop_boxed_16_ = lean_unbox_usize(v_stop_13_);
lean_dec(v_stop_13_);
v_res_17_ = lp_bitblock_x2dproof___private_Init_Data_Array_Basic_0__Array_foldrMUnsafe_fold___at___00__private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0_spec__0(v_as_11_, v_i_boxed_15_, v_stop_boxed_16_, v_b_14_);
lean_dec_ref(v_as_11_);
return v_res_17_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0(lean_object* v_l_18_, lean_object* v_a_19_, lean_object* v_a_20_, lean_object* v_a_21_){
_start:
{
if (lean_obj_tag(v_a_20_) == 0)
{
lean_dec_ref(v_a_21_);
lean_inc(v_l_18_);
return v_l_18_;
}
else
{
lean_object* v_head_22_; lean_object* v_tail_23_; uint8_t v___x_24_; 
v_head_22_ = lean_ctor_get(v_a_20_, 0);
lean_inc(v_head_22_);
v_tail_23_ = lean_ctor_get(v_a_20_, 1);
lean_inc(v_tail_23_);
lean_dec_ref_known(v_a_20_, 2);
v___x_24_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_head_22_, v_a_19_);
if (v___x_24_ == 0)
{
lean_object* v___x_25_; 
v___x_25_ = lean_array_push(v_a_21_, v_head_22_);
v_a_20_ = v_tail_23_;
v_a_21_ = v___x_25_;
goto _start;
}
else
{
lean_object* v___x_27_; lean_object* v___x_28_; uint8_t v___x_29_; 
lean_dec(v_head_22_);
v___x_27_ = lean_array_get_size(v_a_21_);
v___x_28_ = lean_unsigned_to_nat(0u);
v___x_29_ = lean_nat_dec_lt(v___x_28_, v___x_27_);
if (v___x_29_ == 0)
{
lean_dec_ref(v_a_21_);
return v_tail_23_;
}
else
{
size_t v___x_30_; size_t v___x_31_; lean_object* v___x_32_; 
v___x_30_ = lean_usize_of_nat(v___x_27_);
v___x_31_ = ((size_t)0ULL);
v___x_32_ = lp_bitblock_x2dproof___private_Init_Data_Array_Basic_0__Array_foldrMUnsafe_fold___at___00__private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0_spec__0(v_a_21_, v___x_30_, v___x_31_, v_tail_23_);
lean_dec_ref(v_a_21_);
return v___x_32_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0___boxed(lean_object* v_l_33_, lean_object* v_a_34_, lean_object* v_a_35_, lean_object* v_a_36_){
_start:
{
lean_object* v_res_37_; 
v_res_37_ = lp_bitblock_x2dproof___private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0(v_l_33_, v_a_34_, v_a_35_, v_a_36_);
lean_dec_ref(v_a_34_);
lean_dec(v_l_33_);
return v_res_37_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_mutableSupply(lean_object* v_world_40_){
_start:
{
lean_object* v___x_41_; lean_object* v___x_42_; lean_object* v___x_43_; lean_object* v___x_44_; 
v___x_41_ = lp_bitblock_x2dproof_BitblockProof_origin;
v___x_42_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_mutableSupply___closed__0));
lean_inc(v_world_40_);
v___x_43_ = lp_bitblock_x2dproof___private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0(v_world_40_, v___x_41_, v_world_40_, v___x_42_);
lean_dec(v_world_40_);
v___x_44_ = l_List_lengthTR___redArg(v___x_43_);
lean_dec(v___x_43_);
return v___x_44_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_mint___closed__0(void){
_start:
{
lean_object* v___x_45_; lean_object* v___f_46_; 
v___x_45_ = lean_alloc_closure((void*)(lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord___boxed), 2, 0);
v___f_46_ = lean_alloc_closure((void*)(l_instBEqOfDecidableEq___redArg___lam__0___boxed), 3, 1);
lean_closure_set(v___f_46_, 0, v___x_45_);
return v___f_46_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_mint(lean_object* v_world_47_, lean_object* v_coord_48_){
_start:
{
lean_object* v___x_49_; uint8_t v___x_50_; 
v___x_49_ = lp_bitblock_x2dproof_BitblockProof_origin;
v___x_50_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_coord_48_, v___x_49_);
if (v___x_50_ == 0)
{
lean_object* v___f_51_; uint8_t v___x_52_; 
v___f_51_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_mint___closed__0, &lp_bitblock_x2dproof_BitblockProof_mint___closed__0_once, _init_lp_bitblock_x2dproof_BitblockProof_mint___closed__0);
lean_inc(v_world_47_);
lean_inc_ref(v_coord_48_);
v___x_52_ = l_List_elem___redArg(v___f_51_, v_coord_48_, v_world_47_);
if (v___x_52_ == 0)
{
lean_object* v___x_53_; 
v___x_53_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v___x_53_, 0, v_coord_48_);
lean_ctor_set(v___x_53_, 1, v_world_47_);
return v___x_53_;
}
else
{
lean_dec_ref(v_coord_48_);
return v_world_47_;
}
}
else
{
lean_dec_ref(v_coord_48_);
return v_world_47_;
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_burn(lean_object* v_world_54_, lean_object* v_coord_55_){
_start:
{
lean_object* v___x_56_; uint8_t v___x_57_; 
v___x_56_ = lp_bitblock_x2dproof_BitblockProof_origin;
v___x_57_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_coord_55_, v___x_56_);
if (v___x_57_ == 0)
{
lean_object* v___f_58_; uint8_t v___x_59_; 
v___f_58_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_mint___closed__0, &lp_bitblock_x2dproof_BitblockProof_mint___closed__0_once, _init_lp_bitblock_x2dproof_BitblockProof_mint___closed__0);
lean_inc(v_world_54_);
lean_inc_ref(v_coord_55_);
v___x_59_ = l_List_elem___redArg(v___f_58_, v_coord_55_, v_world_54_);
if (v___x_59_ == 0)
{
lean_dec_ref(v_coord_55_);
return v_world_54_;
}
else
{
lean_object* v___x_60_; lean_object* v___x_61_; 
v___x_60_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_mutableSupply___closed__0));
lean_inc(v_world_54_);
v___x_61_ = lp_bitblock_x2dproof___private_Init_Data_List_Impl_0__List_eraseTR_go___at___00BitblockProof_mutableSupply_spec__0(v_world_54_, v_coord_55_, v_world_54_, v___x_60_);
lean_dec_ref(v_coord_55_);
lean_dec(v_world_54_);
return v___x_61_;
}
}
else
{
lean_dec_ref(v_coord_55_);
return v_world_54_;
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorIdx(lean_object* v_x_62_){
_start:
{
if (lean_obj_tag(v_x_62_) == 0)
{
lean_object* v___x_63_; 
v___x_63_ = lean_unsigned_to_nat(0u);
return v___x_63_;
}
else
{
lean_object* v___x_64_; 
v___x_64_ = lean_unsigned_to_nat(1u);
return v___x_64_;
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorIdx___boxed(lean_object* v_x_65_){
_start:
{
lean_object* v_res_66_; 
v_res_66_ = lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorIdx(v_x_65_);
lean_dec_ref(v_x_65_);
return v_res_66_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim___redArg(lean_object* v_t_67_, lean_object* v_k_68_){
_start:
{
lean_object* v_coord_69_; lean_object* v___x_70_; 
v_coord_69_ = lean_ctor_get(v_t_67_, 0);
lean_inc_ref(v_coord_69_);
lean_dec_ref(v_t_67_);
v___x_70_ = lean_apply_1(v_k_68_, v_coord_69_);
return v___x_70_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim(lean_object* v_motive_71_, lean_object* v_ctorIdx_72_, lean_object* v_t_73_, lean_object* v_h_74_, lean_object* v_k_75_){
_start:
{
lean_object* v___x_76_; 
v___x_76_ = lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim___redArg(v_t_73_, v_k_75_);
return v___x_76_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim___boxed(lean_object* v_motive_77_, lean_object* v_ctorIdx_78_, lean_object* v_t_79_, lean_object* v_h_80_, lean_object* v_k_81_){
_start:
{
lean_object* v_res_82_; 
v_res_82_ = lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim(v_motive_77_, v_ctorIdx_78_, v_t_79_, v_h_80_, v_k_81_);
lean_dec(v_ctorIdx_78_);
return v_res_82_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_mint_elim___redArg(lean_object* v_t_83_, lean_object* v_mint_84_){
_start:
{
lean_object* v___x_85_; 
v___x_85_ = lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim___redArg(v_t_83_, v_mint_84_);
return v___x_85_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_mint_elim(lean_object* v_motive_86_, lean_object* v_t_87_, lean_object* v_h_88_, lean_object* v_mint_89_){
_start:
{
lean_object* v___x_90_; 
v___x_90_ = lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim___redArg(v_t_87_, v_mint_89_);
return v___x_90_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_burn_elim___redArg(lean_object* v_t_91_, lean_object* v_burn_92_){
_start:
{
lean_object* v___x_93_; 
v___x_93_ = lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim___redArg(v_t_91_, v_burn_92_);
return v___x_93_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_SupplyEdit_burn_elim(lean_object* v_motive_94_, lean_object* v_t_95_, lean_object* v_h_96_, lean_object* v_burn_97_){
_start:
{
lean_object* v___x_98_; 
v___x_98_ = lp_bitblock_x2dproof_BitblockProof_SupplyEdit_ctorElim___redArg(v_t_95_, v_burn_97_);
return v___x_98_;
}
}
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit_decEq(lean_object* v_x_99_, lean_object* v_x_100_){
_start:
{
if (lean_obj_tag(v_x_99_) == 0)
{
if (lean_obj_tag(v_x_100_) == 0)
{
lean_object* v_coord_101_; lean_object* v_coord_102_; uint8_t v___x_103_; 
v_coord_101_ = lean_ctor_get(v_x_99_, 0);
v_coord_102_ = lean_ctor_get(v_x_100_, 0);
v___x_103_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_coord_101_, v_coord_102_);
return v___x_103_;
}
else
{
uint8_t v___x_104_; 
v___x_104_ = 0;
return v___x_104_;
}
}
else
{
if (lean_obj_tag(v_x_100_) == 0)
{
uint8_t v___x_105_; 
v___x_105_ = 0;
return v___x_105_;
}
else
{
lean_object* v_coord_106_; lean_object* v_coord_107_; uint8_t v___x_108_; 
v_coord_106_ = lean_ctor_get(v_x_99_, 0);
v_coord_107_ = lean_ctor_get(v_x_100_, 0);
v___x_108_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqCoord_decEq(v_coord_106_, v_coord_107_);
return v___x_108_;
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit_decEq___boxed(lean_object* v_x_109_, lean_object* v_x_110_){
_start:
{
uint8_t v_res_111_; lean_object* v_r_112_; 
v_res_111_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit_decEq(v_x_109_, v_x_110_);
lean_dec_ref(v_x_110_);
lean_dec_ref(v_x_109_);
v_r_112_ = lean_box(v_res_111_);
return v_r_112_;
}
}
LEAN_EXPORT uint8_t lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit(lean_object* v_x_113_, lean_object* v_x_114_){
_start:
{
uint8_t v___x_115_; 
v___x_115_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit_decEq(v_x_113_, v_x_114_);
return v___x_115_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit___boxed(lean_object* v_x_116_, lean_object* v_x_117_){
_start:
{
uint8_t v_res_118_; lean_object* v_r_119_; 
v_res_118_ = lp_bitblock_x2dproof_BitblockProof_instDecidableEqSupplyEdit(v_x_116_, v_x_117_);
lean_dec_ref(v_x_117_);
lean_dec_ref(v_x_116_);
v_r_119_ = lean_box(v_res_118_);
return v_r_119_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__3(void){
_start:
{
lean_object* v___x_126_; lean_object* v___x_127_; 
v___x_126_ = lean_unsigned_to_nat(2u);
v___x_127_ = lean_nat_to_int(v___x_126_);
return v___x_127_;
}
}
static lean_object* _init_lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__4(void){
_start:
{
lean_object* v___x_128_; lean_object* v___x_129_; 
v___x_128_ = lean_unsigned_to_nat(1u);
v___x_129_ = lean_nat_to_int(v___x_128_);
return v___x_129_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr(lean_object* v_x_136_, lean_object* v_prec_137_){
_start:
{
if (lean_obj_tag(v_x_136_) == 0)
{
lean_object* v_coord_138_; lean_object* v___y_140_; lean_object* v___x_148_; uint8_t v___x_149_; 
v_coord_138_ = lean_ctor_get(v_x_136_, 0);
v___x_148_ = lean_unsigned_to_nat(1024u);
v___x_149_ = lean_nat_dec_le(v___x_148_, v_prec_137_);
if (v___x_149_ == 0)
{
lean_object* v___x_150_; 
v___x_150_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__3, &lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__3_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__3);
v___y_140_ = v___x_150_;
goto v___jp_139_;
}
else
{
lean_object* v___x_151_; 
v___x_151_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__4, &lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__4_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__4);
v___y_140_ = v___x_151_;
goto v___jp_139_;
}
v___jp_139_:
{
lean_object* v___x_141_; lean_object* v___x_142_; lean_object* v___x_143_; lean_object* v___x_144_; uint8_t v___x_145_; lean_object* v___x_146_; lean_object* v___x_147_; 
v___x_141_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__2));
v___x_142_ = lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg(v_coord_138_);
v___x_143_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_143_, 0, v___x_141_);
lean_ctor_set(v___x_143_, 1, v___x_142_);
lean_inc(v___y_140_);
v___x_144_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_144_, 0, v___y_140_);
lean_ctor_set(v___x_144_, 1, v___x_143_);
v___x_145_ = 0;
v___x_146_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_146_, 0, v___x_144_);
lean_ctor_set_uint8(v___x_146_, sizeof(void*)*1, v___x_145_);
v___x_147_ = l_Repr_addAppParen(v___x_146_, v_prec_137_);
return v___x_147_;
}
}
else
{
lean_object* v_coord_152_; lean_object* v___y_154_; lean_object* v___x_162_; uint8_t v___x_163_; 
v_coord_152_ = lean_ctor_get(v_x_136_, 0);
v___x_162_ = lean_unsigned_to_nat(1024u);
v___x_163_ = lean_nat_dec_le(v___x_162_, v_prec_137_);
if (v___x_163_ == 0)
{
lean_object* v___x_164_; 
v___x_164_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__3, &lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__3_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__3);
v___y_154_ = v___x_164_;
goto v___jp_153_;
}
else
{
lean_object* v___x_165_; 
v___x_165_ = lean_obj_once(&lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__4, &lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__4_once, _init_lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__4);
v___y_154_ = v___x_165_;
goto v___jp_153_;
}
v___jp_153_:
{
lean_object* v___x_155_; lean_object* v___x_156_; lean_object* v___x_157_; lean_object* v___x_158_; uint8_t v___x_159_; lean_object* v___x_160_; lean_object* v___x_161_; 
v___x_155_ = ((lean_object*)(lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___closed__7));
v___x_156_ = lp_bitblock_x2dproof_BitblockProof_instReprCoord_repr___redArg(v_coord_152_);
v___x_157_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_157_, 0, v___x_155_);
lean_ctor_set(v___x_157_, 1, v___x_156_);
lean_inc(v___y_154_);
v___x_158_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_158_, 0, v___y_154_);
lean_ctor_set(v___x_158_, 1, v___x_157_);
v___x_159_ = 0;
v___x_160_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_160_, 0, v___x_158_);
lean_ctor_set_uint8(v___x_160_, sizeof(void*)*1, v___x_159_);
v___x_161_ = l_Repr_addAppParen(v___x_160_, v_prec_137_);
return v___x_161_;
}
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr___boxed(lean_object* v_x_166_, lean_object* v_prec_167_){
_start:
{
lean_object* v_res_168_; 
v_res_168_ = lp_bitblock_x2dproof_BitblockProof_instReprSupplyEdit_repr(v_x_166_, v_prec_167_);
lean_dec(v_prec_167_);
lean_dec_ref(v_x_166_);
return v_res_168_;
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_applySupplyEdit(lean_object* v_world_171_, lean_object* v_x_172_){
_start:
{
if (lean_obj_tag(v_x_172_) == 0)
{
lean_object* v_coord_173_; lean_object* v___x_174_; 
v_coord_173_ = lean_ctor_get(v_x_172_, 0);
lean_inc_ref(v_coord_173_);
lean_dec_ref_known(v_x_172_, 1);
v___x_174_ = lp_bitblock_x2dproof_BitblockProof_mint(v_world_171_, v_coord_173_);
return v___x_174_;
}
else
{
lean_object* v_coord_175_; lean_object* v___x_176_; 
v_coord_175_ = lean_ctor_get(v_x_172_, 0);
lean_inc_ref(v_coord_175_);
lean_dec_ref_known(v_x_172_, 1);
v___x_176_ = lp_bitblock_x2dproof_BitblockProof_burn(v_world_171_, v_coord_175_);
return v___x_176_;
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof_BitblockProof_replaySupply(lean_object* v_x_177_, lean_object* v_x_178_){
_start:
{
if (lean_obj_tag(v_x_178_) == 0)
{
return v_x_177_;
}
else
{
lean_object* v_head_179_; lean_object* v_tail_180_; lean_object* v___x_181_; 
v_head_179_ = lean_ctor_get(v_x_178_, 0);
lean_inc(v_head_179_);
v_tail_180_ = lean_ctor_get(v_x_178_, 1);
lean_inc(v_tail_180_);
lean_dec_ref_known(v_x_178_, 2);
v___x_181_ = lp_bitblock_x2dproof_BitblockProof_applySupplyEdit(v_x_177_, v_head_179_);
v_x_177_ = v___x_181_;
v_x_178_ = v_tail_180_;
goto _start;
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_MintBurn_0__BitblockProof_instReprSupplyEdit_repr_match__1_splitter___redArg(lean_object* v_x_183_, lean_object* v_h__1_184_, lean_object* v_h__2_185_){
_start:
{
if (lean_obj_tag(v_x_183_) == 0)
{
lean_object* v_coord_186_; lean_object* v___x_187_; 
lean_dec(v_h__2_185_);
v_coord_186_ = lean_ctor_get(v_x_183_, 0);
lean_inc_ref(v_coord_186_);
lean_dec_ref_known(v_x_183_, 1);
v___x_187_ = lean_apply_1(v_h__1_184_, v_coord_186_);
return v___x_187_;
}
else
{
lean_object* v_coord_188_; lean_object* v___x_189_; 
lean_dec(v_h__1_184_);
v_coord_188_ = lean_ctor_get(v_x_183_, 0);
lean_inc_ref(v_coord_188_);
lean_dec_ref_known(v_x_183_, 1);
v___x_189_ = lean_apply_1(v_h__2_185_, v_coord_188_);
return v___x_189_;
}
}
}
LEAN_EXPORT lean_object* lp_bitblock_x2dproof___private_BitblockProof_MintBurn_0__BitblockProof_instReprSupplyEdit_repr_match__1_splitter(lean_object* v_motive_190_, lean_object* v_x_191_, lean_object* v_h__1_192_, lean_object* v_h__2_193_){
_start:
{
if (lean_obj_tag(v_x_191_) == 0)
{
lean_object* v_coord_194_; lean_object* v___x_195_; 
lean_dec(v_h__2_193_);
v_coord_194_ = lean_ctor_get(v_x_191_, 0);
lean_inc_ref(v_coord_194_);
lean_dec_ref_known(v_x_191_, 1);
v___x_195_ = lean_apply_1(v_h__1_192_, v_coord_194_);
return v___x_195_;
}
else
{
lean_object* v_coord_196_; lean_object* v___x_197_; 
lean_dec(v_h__1_192_);
v_coord_196_ = lean_ctor_get(v_x_191_, 0);
lean_inc_ref(v_coord_196_);
lean_dec_ref_known(v_x_191_, 1);
v___x_197_ = lean_apply_1(v_h__2_193_, v_coord_196_);
return v___x_197_;
}
}
}
lean_object* initialize_Init(uint8_t builtin);
lean_object* initialize_Init(uint8_t builtin);
lean_object* initialize_bitblock_x2dproof_BitblockProof_World(uint8_t builtin);
static bool _G_initialized = false;
LEAN_EXPORT lean_object* initialize_bitblock_x2dproof_BitblockProof_MintBurn(uint8_t builtin) {
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
return lean_io_result_mk_ok(lean_box(0));
}
#ifdef __cplusplus
}
#endif
