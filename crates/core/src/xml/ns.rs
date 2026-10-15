// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Namespace URIs the engine reads or writes.

/// WordprocessingML (transitional).
pub const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
/// WordprocessingML (strict), which this version does not support.
pub const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
/// Word 2010 extensions (`w14:paraId`, `w14:textId`).
pub const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
/// Relationship references inside parts (`r:id`, `r:embed`).
pub const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// Markup compatibility (`mc:AlternateContent`, `mc:Ignorable`).
pub const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
/// Math.
pub const M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
/// DrawingML main.
pub const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// `[Content_Types].xml`.
pub const CONTENT_TYPES: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
/// `*.rels` parts.
pub const PACKAGE_RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
/// `docProps/custom.xml`.
pub const CUSTOM_PROPS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties";
/// Variant types inside custom properties.
pub const VT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes";
/// `docProps/core.xml`.
pub const CORE_PROPS: &str =
    "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
/// `docProps/app.xml`.
pub const EXTENDED_PROPS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties";

/// Relationship type prefix for parts inside a document.
pub const REL_OFFICE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// Relationship type of the custom properties part.
pub const REL_CUSTOM_PROPS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties";
/// Relationship type of an external hyperlink.
pub const REL_HYPERLINK: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";

/// Content type of the main part of a .docx.
pub const CT_DOCUMENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
/// Content type of the main part of a .docm.
pub const CT_DOCUMENT_MACRO: &str = "application/vnd.ms-word.document.macroEnabled.main+xml";
/// Content type of the main part of a .dotx.
pub const CT_TEMPLATE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml";
/// Content type of `docProps/custom.xml`.
pub const CT_CUSTOM_PROPS: &str =
    "application/vnd.openxmlformats-officedocument.custom-properties+xml";
