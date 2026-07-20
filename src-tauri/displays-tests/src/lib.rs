//! Andamio para correr los tests de los archivos reales de `src/displays/`.
//!
//! Ver el `Cargo.toml` de al lado para el porqué. Acá solo se reproduce el
//! entorno que esos archivos esperan: `crate::runtime_log`, `crate::json_store`
//! y `super::diagnostics`.
//!
//! **Los `#[path]` son relativos**, no absolutos: tienen que funcionar igual en
//! la máquina del dueño y en el runner del CI. Ojo con el nivel: adentro de
//! `mod displays { ... }` la base pasa a ser `src/displays/`, no `src/`, así que
//! los de adentro llevan un `../` más que el de `json_store`.
//!
//! El `allow(dead_code)` es del andamio, no del código real: acá nadie *usa* el
//! watchdog fuera de sus tests, así que sin esto cada corrida escupe una decena
//! de "never used" que no significan nada. En el crate de la app esos mismos
//! archivos sí tienen consumidor, y ahí las advertencias siguen valiendo.
#![allow(dead_code)]

/// Doble del `runtime_log` de Millennium. Misma firma exacta —`impl Into<String>`,
/// y `err`, no `error`—, así un cambio de firma allá rompe acá y se nota.
pub mod runtime_log {
    pub fn info(msg: impl Into<String>) {
        let _ = msg.into();
    }
    pub fn warn(msg: impl Into<String>) {
        let _ = msg.into();
    }
    pub fn err(msg: impl Into<String>) {
        let _ = msg.into();
    }
}

// El store atómico real: `displays::store` se apoya en él, y de paso sus propios
// tests entran al gate.
#[path = "../../src/json_store.rs"]
pub mod json_store;

pub mod displays {
    /// Doble del shim de logging de `displays/mod.rs` (misma firma exacta).
    pub(crate) mod diagnostics {
        pub fn log(message: impl AsRef<str>) {
            let _ = message.as_ref();
        }
    }

    #[path = "../../../src/displays/ids.rs"]
    pub mod ids;

    #[path = "../../../src/displays/store.rs"]
    pub mod store;

    #[path = "../../../src/displays/watchdog.rs"]
    pub mod watchdog;
}
