//! DirectWrite text system.
//!
//! - **Fallback.** Segoe UI Variable has no Hangul. Rather than relying on
//!   whatever the system fallback picks, a custom fallback maps Hangul and
//!   CJK punctuation ranges to the theme's fallback families (normally
//!   Malgun Gothic) and then defers to the system fallback for everything
//!   else (emoji, other scripts).
//! - **Weights.** Segoe UI Variable is a variable font with no named 500
//!   instance. Without an explicit axis value DirectWrite snaps 500 to
//!   Semibold (600), so the weight axis is set to the exact token value.
//! - **Antialiasing.** Grayscale (set on the Direct2D context): ClearType
//!   needs an opaque target, and the panel surface may be translucent.
//!
//! Everything here is device-independent and survives device loss.

use std::cell::RefCell;
use std::collections::HashMap;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_AXIS_TAG_WEIGHT, DWRITE_FONT_AXIS_VALUE,
    DWRITE_FONT_FAMILY_MODEL_TYPOGRAPHIC, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_READING_DIRECTION,
    DWRITE_READING_DIRECTION_LEFT_TO_RIGHT, DWRITE_TEXT_METRICS, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_UNICODE_RANGE, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWriteCreateFactory, IDWriteFactory2, IDWriteFactory6, IDWriteFont, IDWriteFontCollection,
    IDWriteFontFallback, IDWriteNumberSubstitution, IDWriteTextAnalysisSource,
    IDWriteTextAnalysisSource_Impl, IDWriteTextFormat, IDWriteTextFormat1, IDWriteTextFormat3,
    IDWriteTextLayout,
};
use windows::core::{BOOL, HSTRING, Interface, PCWSTR, Result, implement};
use windowsrice_ui::Size;
use windowsrice_ui::draw::{FontRole, TextStyle};
use windowsrice_ui::text::TextMeasure;

/// Font families requested by the theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontConfig {
    pub family: String,
    pub fallback: Vec<String>,
    pub icon_family: String,
    pub icon_fallback: String,
    /// BCP 47 locale for shaping, e.g. `ko-KR`.
    pub locale: String,
}

/// One installed font family (see [`TextSystem::installed_families`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledFamily {
    /// English (or first available) family name.
    pub name: String,
    /// Weights of the upright faces, ascending.
    pub weights: Vec<u16>,
    /// The family itself has Hangul syllables (가, 한).
    pub hangul: bool,
    /// A symbol font (icons, dingbats): not offered for text.
    pub symbol: bool,
    /// Fixed-width family (code fonts).
    pub mono: bool,
}

/// English name of a family, else its first localized name.
fn family_name(fam: &windows::Win32::Graphics::DirectWrite::IDWriteFontFamily) -> Option<String> {
    // SAFETY: reads localized strings of a valid family into owned buffers.
    unsafe {
        let names = fam.GetFamilyNames().ok()?;
        let mut index = 0u32;
        let mut exists = BOOL(0);
        names
            .FindLocaleName(&HSTRING::from("en-us"), &raw mut index, &raw mut exists)
            .ok()?;
        if !exists.as_bool() {
            index = 0;
        }
        let len = names.GetStringLength(index).ok()?;
        let mut buf = vec![0u16; len as usize + 1];
        names.GetString(index, &mut buf).ok()?;
        buf.truncate(len as usize);
        String::from_utf16(&buf).ok()
    }
}

/// Unicode ranges routed to the theme's fallback families first.
const HANGUL_AND_CJK: [DWRITE_UNICODE_RANGE; 7] = [
    DWRITE_UNICODE_RANGE {
        first: 0x1100,
        last: 0x11FF,
    }, // Hangul Jamo
    DWRITE_UNICODE_RANGE {
        first: 0x3000,
        last: 0x303F,
    }, // CJK symbols and punctuation
    DWRITE_UNICODE_RANGE {
        first: 0x3130,
        last: 0x318F,
    }, // Hangul compatibility Jamo
    DWRITE_UNICODE_RANGE {
        first: 0xA960,
        last: 0xA97F,
    }, // Hangul Jamo extended-A
    DWRITE_UNICODE_RANGE {
        first: 0xAC00,
        last: 0xD7AF,
    }, // Hangul syllables
    DWRITE_UNICODE_RANGE {
        first: 0xD7B0,
        last: 0xD7FF,
    }, // Hangul Jamo extended-B
    DWRITE_UNICODE_RANGE {
        first: 0xFF00,
        last: 0xFFEF,
    }, // half-width and full-width forms
];

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FormatKey {
    font: FontRole,
    size_centi: u32,
    weight: u16,
    trim: bool,
}

pub struct TextSystem {
    factory: IDWriteFactory2,
    /// System fonts in the typographic family model, where variable fonts
    /// such as "Segoe UI Variable" appear as one family.
    collection: IDWriteFontCollection,
    fallback: IDWriteFontFallback,
    config: FontConfig,
    icon_family: String,
    locale: HSTRING,
    formats: RefCell<HashMap<FormatKey, IDWriteTextFormat>>,
    /// Families that could not be found when the system was created.
    pub missing_families: Vec<String>,
}

impl std::fmt::Debug for TextSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextSystem")
            .field("config", &self.config)
            .field("icon_family", &self.icon_family)
            .field("missing_families", &self.missing_families)
            .finish_non_exhaustive()
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

impl TextSystem {
    pub fn new(config: FontConfig) -> Result<Self> {
        // SAFETY: DirectWrite factory and fallback construction; all strings
        // passed are NUL-terminated buffers that outlive the calls.
        unsafe {
            let factory6: IDWriteFactory6 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let collection: IDWriteFontCollection = factory6
                .GetSystemFontCollection(false, DWRITE_FONT_FAMILY_MODEL_TYPOGRAPHIC)?
                .cast()?;
            let factory: IDWriteFactory2 = factory6.cast()?;
            let exists = |name: &str| -> bool {
                let mut index = 0u32;
                let mut found = BOOL(0);
                collection
                    .FindFamilyName(&HSTRING::from(name), &raw mut index, &raw mut found)
                    .is_ok()
                    && found.as_bool()
            };
            let mut missing = Vec::new();
            for f in std::iter::once(&config.family).chain(&config.fallback) {
                if !exists(f) {
                    missing.push(f.clone());
                }
            }
            let icon_family = if exists(&config.icon_family) {
                config.icon_family.clone()
            } else {
                missing.push(config.icon_family.clone());
                config.icon_fallback.clone()
            };

            let builder = factory.CreateFontFallbackBuilder()?;
            let names: Vec<Vec<u16>> = config
                .fallback
                .iter()
                .filter(|f| exists(f))
                .map(|f| wide(f))
                .collect();
            if !names.is_empty() {
                let ptrs: Vec<*const u16> = names.iter().map(|n| n.as_ptr()).collect();
                builder.AddMapping(
                    &HANGUL_AND_CJK,
                    &ptrs,
                    &collection,
                    PCWSTR::null(),
                    PCWSTR::null(),
                    1.0,
                )?;
            }
            builder.AddMappings(&factory.GetSystemFontFallback()?)?;
            let fallback = builder.CreateFontFallback()?;

            Ok(Self {
                locale: HSTRING::from(config.locale.as_str()),
                factory,
                collection,
                fallback,
                icon_family,
                config,
                formats: RefCell::new(HashMap::new()),
                missing_families: missing,
            })
        }
    }

    pub fn icon_family(&self) -> &str {
        &self.icon_family
    }

    fn format(&self, style: &TextStyle, trim: bool) -> Result<IDWriteTextFormat> {
        let key = FormatKey {
            font: style.font,
            size_centi: (style.size * 100.0).round() as u32,
            weight: style.weight,
            trim,
        };
        if let Some(f) = self.formats.borrow().get(&key) {
            return Ok(f.clone());
        }
        let family = match style.font {
            FontRole::Ui => self.config.family.as_str(),
            FontRole::Icon => self.icon_family.as_str(),
            FontRole::Family(f) => f,
        };
        // SAFETY: DirectWrite calls with owned strings; out-parameters valid.
        let format = unsafe {
            let f = self.factory.CreateTextFormat(
                &HSTRING::from(family),
                &self.collection,
                DWRITE_FONT_WEIGHT(i32::from(style.weight)),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                style.size,
                &self.locale,
            )?;
            f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            if matches!(style.font, FontRole::Ui | FontRole::Family(_)) {
                f.cast::<IDWriteTextFormat1>()?
                    .SetFontFallback(&self.fallback)?;
                if let Ok(f3) = f.cast::<IDWriteTextFormat3>() {
                    f3.SetFontAxisValues(&[DWRITE_FONT_AXIS_VALUE {
                        axisTag: DWRITE_FONT_AXIS_TAG_WEIGHT,
                        value: f32::from(style.weight),
                    }])?;
                }
            }
            if trim {
                let sign = self.factory.CreateEllipsisTrimmingSign(&f)?;
                let t = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                };
                f.SetTrimming(&raw const t, &sign)?;
            }
            f
        };
        self.formats.borrow_mut().insert(key, format.clone());
        Ok(format)
    }

    /// Lays out one line of text in a `max_w` × `h` DIP box.
    pub fn layout(
        &self,
        text: &str,
        style: &TextStyle,
        max_w: f32,
        h: f32,
        trim: bool,
    ) -> Result<IDWriteTextLayout> {
        let format = self.format(style, trim)?;
        let w: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: valid format and UTF-16 slice.
        unsafe {
            self.factory
                .CreateTextLayout(&w, &format, max_w.max(0.0), h.max(0.0))
        }
    }

    /// Lays out text wrapped at word boundaries, top-aligned, in a
    /// w × h DIP box.
    pub fn layout_wrapped(
        &self,
        text: &str,
        style: &TextStyle,
        w: f32,
        h: f32,
    ) -> Result<IDWriteTextLayout> {
        let l = self.layout(text, style, w, h, false)?;
        // SAFETY: setting properties of our own layout.
        unsafe {
            l.SetWordWrapping(windows::Win32::Graphics::DirectWrite::DWRITE_WORD_WRAPPING_WRAP)?;
            l.SetParagraphAlignment(
                windows::Win32::Graphics::DirectWrite::DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
            )?;
        }
        Ok(l)
    }

    /// Family DirectWrite actually uses for the first character of `text`
    /// with the UI font at `weight` (diagnostics and tests).
    pub fn resolved_family(&self, text: &str, weight: u16) -> Result<Option<String>> {
        let source: IDWriteTextAnalysisSource = Source::new(text, &self.config.locale).into();
        let len = u32::try_from(text.encode_utf16().count()).unwrap_or(0);
        let mut mapped_len = 0u32;
        let mut font: Option<IDWriteFont> = None;
        let mut scale = 1.0f32;
        // SAFETY: the analysis source covers `len` UTF-16 units; out-pointers
        // are locals.
        unsafe {
            self.fallback.MapCharacters(
                &source,
                0,
                len,
                &self.collection,
                &HSTRING::from(self.config.family.as_str()),
                DWRITE_FONT_WEIGHT(i32::from(weight)),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                &raw mut mapped_len,
                &raw mut font,
                &raw mut scale,
            )?;
            let Some(font) = font else {
                // No fallback needed: the base family covers the text.
                return Ok(Some(self.config.family.clone()));
            };
            let names = font.GetFontFamily()?.GetFamilyNames()?;
            let mut buf = [0u16; 128];
            names.GetString(0, &mut buf)?;
            let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Ok(Some(String::from_utf16_lossy(&buf[..n])))
        }
    }

    /// Whether amily is installed and, if so, whether it has Hangul
    /// syllables (otherwise Hangul is drawn by the fallback family).
    pub fn family_info(&self, family: &str) -> Option<bool> {
        let mut index = 0u32;
        let mut found = BOOL(0);
        // SAFETY: lookups in our own font collection with owned strings.
        unsafe {
            self.collection
                .FindFamilyName(&HSTRING::from(family), &raw mut index, &raw mut found)
                .ok()?;
            if !found.as_bool() {
                return None;
            }
            let fam = self.collection.GetFontFamily(index).ok()?;
            let font = fam.GetFont(0).ok()?;
            Some(font.HasCharacter(u32::from('가')).ok()?.as_bool())
        }
    }

    /// Every installed font family, from DirectWrite's typographic system
    /// collection (so "Segoe UI Variable" is one family, not one per optical
    /// size). Sorted by name, without duplicates. This walks every font face,
    /// so callers cache the result instead of calling it per frame.
    pub fn installed_families(&self) -> Vec<InstalledFamily> {
        let mut out: Vec<InstalledFamily> = Vec::new();
        // SAFETY: read-only enumeration of our own font collection.
        unsafe {
            let count = self.collection.GetFontFamilyCount();
            for i in 0..count {
                let Ok(fam) = self.collection.GetFontFamily(i) else {
                    continue;
                };
                let Some(name) = family_name(&fam) else {
                    continue;
                };
                let mut weights: Vec<u16> = Vec::new();
                let (mut hangul, mut symbol, mut mono) = (false, false, false);
                for j in 0..fam.GetFontCount() {
                    let Ok(font) = fam.GetFont(j) else { continue };
                    if font.GetStyle() != DWRITE_FONT_STYLE_NORMAL {
                        continue;
                    }
                    if let Ok(w) = u16::try_from(font.GetWeight().0) {
                        weights.push(w);
                    }
                    symbol |= font.IsSymbolFont().as_bool();
                    mono |= font
                        .cast::<windows::Win32::Graphics::DirectWrite::IDWriteFont1>()
                        .is_ok_and(|f1| f1.IsMonospacedFont().as_bool());
                    hangul |= font
                        .HasCharacter(u32::from('가'))
                        .is_ok_and(|b| b.as_bool())
                        && font
                            .HasCharacter(u32::from('한'))
                            .is_ok_and(|b| b.as_bool());
                }
                if weights.is_empty() {
                    continue;
                }
                weights.sort_unstable();
                weights.dedup();
                if let Some(e) = out.iter_mut().find(|e| e.name.eq_ignore_ascii_case(&name)) {
                    e.weights.extend(weights);
                    e.weights.sort_unstable();
                    e.weights.dedup();
                    e.hangul |= hangul;
                    e.symbol |= symbol;
                    e.mono |= mono;
                } else {
                    out.push(InstalledFamily {
                        name,
                        weights,
                        hangul,
                        symbol,
                        mono,
                    });
                }
            }
        }
        out.sort_by_key(|e| e.name.to_lowercase());
        out
    }

    /// Weight of the font DirectWrite selects for `text` in a layout.
    pub fn resolved_weight(&self, text: &str, weight: u16) -> Result<i32> {
        let source: IDWriteTextAnalysisSource = Source::new(text, &self.config.locale).into();
        let len = u32::try_from(text.encode_utf16().count()).unwrap_or(0);
        let mut mapped_len = 0u32;
        let mut font: Option<IDWriteFont> = None;
        let mut scale = 1.0f32;
        // SAFETY: as in `resolved_family`.
        unsafe {
            self.fallback.MapCharacters(
                &source,
                0,
                len,
                &self.collection,
                &HSTRING::from(self.config.family.as_str()),
                DWRITE_FONT_WEIGHT(i32::from(weight)),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                &raw mut mapped_len,
                &raw mut font,
                &raw mut scale,
            )?;
            Ok(font.map_or(i32::from(weight), |f| f.GetWeight().0))
        }
    }
}

impl TextMeasure for TextSystem {
    fn measure(&self, text: &str, style: &TextStyle) -> Size {
        let Ok(layout) = self.layout(text, style, 100_000.0, style.size * 2.0, false) else {
            return Size::default();
        };
        let mut m = DWRITE_TEXT_METRICS::default();
        // SAFETY: valid layout and out-parameter.
        if unsafe { layout.GetMetrics(&raw mut m) }.is_err() {
            return Size::default();
        }
        Size {
            w: m.widthIncludingTrailingWhitespace.ceil(),
            h: m.height,
        }
    }
}

/// Minimal text analysis source for `IDWriteFontFallback::MapCharacters`.
#[implement(IDWriteTextAnalysisSource)]
struct Source {
    text: Vec<u16>,
    locale: Vec<u16>,
}

impl Source {
    fn new(text: &str, locale: &str) -> Self {
        Self {
            text: text.encode_utf16().collect(),
            locale: wide(locale),
        }
    }
}

impl IDWriteTextAnalysisSource_Impl for Source_Impl {
    fn GetTextAtPosition(&self, pos: u32, text: *mut *mut u16, len: *mut u32) -> Result<()> {
        let pos = pos as usize;
        // SAFETY: DirectWrite passes valid out-pointers; the returned pointer
        // stays valid while this object lives.
        unsafe {
            if pos < self.text.len() {
                *text = self.text.as_ptr().add(pos).cast_mut();
                *len = u32::try_from(self.text.len() - pos).unwrap_or(0);
            } else {
                *text = std::ptr::null_mut();
                *len = 0;
            }
        }
        Ok(())
    }

    fn GetTextBeforePosition(&self, pos: u32, text: *mut *mut u16, len: *mut u32) -> Result<()> {
        let pos = pos as usize;
        // SAFETY: as above.
        unsafe {
            if pos > 0 && pos <= self.text.len() {
                *text = self.text.as_ptr().cast_mut();
                *len = u32::try_from(pos).unwrap_or(0);
            } else {
                *text = std::ptr::null_mut();
                *len = 0;
            }
        }
        Ok(())
    }

    fn GetParagraphReadingDirection(&self) -> DWRITE_READING_DIRECTION {
        DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
    }

    fn GetLocaleName(&self, pos: u32, len: *mut u32, locale: *mut *mut u16) -> Result<()> {
        // SAFETY: valid out-pointers; the locale buffer lives with `self`.
        unsafe {
            *len = u32::try_from(self.text.len().saturating_sub(pos as usize)).unwrap_or(0);
            *locale = self.locale.as_ptr().cast_mut();
        }
        Ok(())
    }

    fn GetNumberSubstitution(
        &self,
        pos: u32,
        len: *mut u32,
        substitution: windows::core::OutRef<'_, IDWriteNumberSubstitution>,
    ) -> Result<()> {
        // SAFETY: valid out-pointer.
        unsafe {
            *len = u32::try_from(self.text.len().saturating_sub(pos as usize)).unwrap_or(0);
        }
        substitution.write(None)
    }
}
