//! The provider retry's startup settings. The retry itself, which answers
//! are worth another send and how long to wait, is `guffbolt_ai::wire`'s.

pub(crate) mod settings;

#[cfg(test)]
mod tests;
