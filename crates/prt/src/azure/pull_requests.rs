//! Fixture de compatibilidade para um teste source-level legado.
//!
//! Este arquivo NÃO é compilado nem contém a integração Azure usada em
//! runtime. A implementação real vive em `../integrations/azure/pull_requests.rs`.
//! O teste em `features/update_pull_request.rs` ainda procura estes símbolos
//! como texto para garantir que o fluxo de update não atravesse o publisher de
//! criação. Quando esse teste for convertido para uma asserção comportamental,
//! este fixture pode ser removido.

// Marcador esperado pelo teste: create_pull_request
// Marcador esperado pelo teste: pub async fn update_pull_request
