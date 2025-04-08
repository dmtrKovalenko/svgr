/// Newtype implementing correct to tokens for the numbes that might
/// be negative to avoid LSP errors because rust analyzer is not able to
/// correctly align the additional - token leading to the unnecessary lsp errors.
#[derive(Debug, Clone, Copy)]
pub struct TokenizableNumber(pub f64);

impl quote::ToTokens for TokenizableNumber {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let TokenizableNumber(number) = self;

        if number.is_sign_negative() {
            let number = number.abs();
            quote::quote! { - #number }.to_tokens(tokens)
        } else {
            quote::quote! { #number }.to_tokens(tokens)
        }
    }
}
