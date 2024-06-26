use crate::derive_data::StructField;
use crate::field_attributes::DefaultBehavior;
use crate::{
    derive_data::{EnumVariantFields, ReflectEnum},
    utility::ident_or_index,
};
use bevy_macro_utils::fq_std::{FQDefault, FQOption};
use proc_macro2::Ident;
use quote::{quote, ToTokens};
use syn::Member;

/// Contains all data needed to construct all variants within an enum.
pub(crate) struct EnumVariantConstructors {
    /// The names of each variant as a string.
    pub variant_names: Vec<String>,
    /// The stream of tokens that will construct each variant.
    pub variant_constructors: Vec<proc_macro2::TokenStream>,
}

/// Gets the constructors for all variants in the given enum.
pub(crate) fn get_variant_constructors(
    reflect_enum: &ReflectEnum,
    ref_value: &Ident,
    return_apply_error: bool,
) -> EnumVariantConstructors {
    let bevy_reflect_path = reflect_enum.meta().bevy_reflect_path();
    let variant_count = reflect_enum.variants().len();
    let mut variant_names = Vec::with_capacity(variant_count);
    let mut variant_constructors = Vec::with_capacity(variant_count);

    for variant in reflect_enum.variants() {
        let ident = &variant.data.ident;
        let name = ident.to_string();
        let variant_constructor = reflect_enum.get_unit(ident);

        let fields: &[StructField] = match &variant.fields {
            EnumVariantFields::Unit => &[],
            EnumVariantFields::Named(fields) | EnumVariantFields::Unnamed(fields) => {
                fields.as_slice()
            }
        };
        let mut reflect_index: usize = 0;
        let constructor_fields = fields.iter().enumerate().map(|(declare_index, field)| {
            let field_ident = ident_or_index(field.data.ident.as_ref(), declare_index);
            let field_ty = &field.data.ty;

            let field_value = if field.attrs.ignore.is_ignored() {
                match &field.attrs.default {
                    DefaultBehavior::Func(path) => quote! { #path() },
                    _ => quote! { #FQDefault::default() }
                }
            } else {
                let field_accessor = match &field.data.ident {
                    Some(ident) => {
                        let name = ident.to_string();
                        quote!(#ref_value.field(#name))
                    }
                    None => quote!(#ref_value.field_at(#reflect_index)),
                };
                reflect_index += 1;

                let (resolve_error, resolve_missing) = if return_apply_error {
                    let field_ref_str = match &field_ident {
                        Member::Named(ident) => format!("{ident}"),
                        Member::Unnamed(index) => format!(".{}", index.index)
                    };
                    let ty = field.data.ty.to_token_stream();

                    (
                        quote!(.ok_or(#bevy_reflect_path::ApplyError::MismatchedTypes {
                            // The unwrap won't panic. By this point the #field_accessor would have been invoked once and any failure to
                            // access the given field handled by the `resolve_missing` code bellow.
                            from_type: ::core::convert::Into::into(
                                #bevy_reflect_path::DynamicTypePath::reflect_type_path(#FQOption::unwrap(#field_accessor))
                            ),
                            to_type: ::core::convert::Into::into(<#ty as #bevy_reflect_path::TypePath>::type_path())
                        })?),
                        quote!(.ok_or(#bevy_reflect_path::ApplyError::MissingEnumField {
                                variant_name: ::core::convert::Into::into(#name),
                                field_name: ::core::convert::Into::into(#field_ref_str)
                        })?)
                    )
                } else {
                    (quote!(?), quote!(?))
                };

                match &field.attrs.default {
                    DefaultBehavior::Func(path) => quote! {
                        if let #FQOption::Some(field) = #field_accessor {
                            <#field_ty as #bevy_reflect_path::FromReflect>::from_reflect(field)
                            #resolve_error
                        } else {
                            #path()
                        }
                    },
                    DefaultBehavior::Default => quote! {
                        if let #FQOption::Some(field) = #field_accessor {
                            <#field_ty as #bevy_reflect_path::FromReflect>::from_reflect(field)
                            #resolve_error
                        } else {
                            #FQDefault::default()
                        }
                    },
                    DefaultBehavior::Required => quote! {
                        <#field_ty as #bevy_reflect_path::FromReflect>::from_reflect(#field_accessor #resolve_missing)
                        #resolve_error
                    },
                }
            };
            quote! { #field_ident : #field_value }
        });
        variant_constructors.push(quote! {
            #variant_constructor { #( #constructor_fields ),* }
        });
        variant_names.push(name);
    }

    EnumVariantConstructors {
        variant_names,
        variant_constructors,
    }
}
