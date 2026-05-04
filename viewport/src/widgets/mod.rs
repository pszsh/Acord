//! Reusable iced widgets pulled out of the Acord editor for use in other apps.
//!
//! Each submodule is generic over the host's `Message` type and renders against
//! the `iced_wgpu::Renderer`. The widgets read from the global Acord palette
//! (see [`crate::palette`]) so callers can theme them by calling
//! `palette::set_theme(...)` before building their UI.

pub mod dialog;
pub mod menu;
pub mod style;
