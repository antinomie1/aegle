use std::ops::Range;
use std::sync::Arc;

use crate::{
    Alignment, ContentWidths, FontFamily, GenericFamily, Paragraph, TextDiagnostics, TextError,
    TextStyle,
    layout::{Layout, PlatformSystem},
    style::Shaping,
};
#[cfg(any(feature = "text-parley", not(target_os = "linux")))]
use aegle_types::Color;
#[cfg(any(feature = "text-parley", not(target_os = "linux")))]
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
#[cfg(any(feature = "text-parley", not(target_os = "linux")))]
use parley::{FontContext, LayoutContext};

/// Font collection and reusable shaping scratch for one UI thread.
///
/// Context scratch grows to accommodate shaped text. Call [`Self::trim`] under
/// memory pressure to release it. Font metadata and fonts referenced by live
/// paragraphs remain owned; this is not a byte-budgeted glyph bitmap cache.
///
/// One system is shared by every window of an application, so it also holds
/// the application-wide budget for idle text, see [`Self::set_idle_budget`].
pub struct TextSystem {
    #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
    pub(crate) fonts: FontContext,
    #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
    pub(crate) context: LayoutContext<Color>,
    pub(crate) backend: PlatformSystem,
    /// Family lists retained text shapes with, shared instead of copied.
    families: Vec<Arc<str>>,
    idle_budget: Option<usize>,
    idle_bytes: usize,
}

impl TextSystem {
    /// Creates an empty font collection without system discovery.
    ///
    /// Register fonts and set generic families or use an explicit family name in
    /// [`TextStyle::families`] before shaping. This behavior does not change when
    /// Cargo feature unification enables `system-fonts` elsewhere in a program.
    pub fn new() -> Self {
        Self::with_system_fonts(false)
    }

    /// Creates a collection with the platform's installed fonts and fallbacks.
    ///
    /// Linux system discovery requires Fontconfig. Font bytes are loaded on
    /// demand, and mapped font files still contribute to process memory use.
    #[cfg(feature = "system-fonts")]
    pub fn system() -> Self {
        Self::with_system_fonts(true)
    }

    fn with_system_fonts(_system_fonts: bool) -> Self {
        Self {
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            fonts: FontContext {
                collection: Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts: _system_fonts,
                }),
                source_cache: SourceCache::default(),
            },
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            context: LayoutContext::new(),
            backend: PlatformSystem::new(_system_fonts),
            families: Vec::new(),
            idle_budget: None,
            idle_bytes: 0,
        }
    }

    /// Registers all readable faces from shared font bytes, without copying
    /// them, and returns the families they belong to. Glyph records keep the
    /// bytes shared. Unsupported or invalid faces are ignored; zero
    /// registered faces is an error.
    pub fn register_fonts(
        &mut self,
        _data: Arc<dyn AsRef<[u8]> + Send + Sync>,
    ) -> Result<Vec<FontFamily>, TextError> {
        #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
        {
            let registered = self.fonts.collection.register_fonts(Blob::new(_data), None);
            if registered.is_empty() {
                return Err(TextError::InvalidFont);
            }
            Ok(registered
                .into_iter()
                .map(|(id, _)| FontFamily(id))
                .collect())
        }
        #[cfg(not(any(feature = "text-parley", not(target_os = "linux"))))]
        {
            self.backend.register_fonts(_data.as_ref().as_ref())
        }
    }

    /// Makes `families`, in order, what a generic family such as
    /// `sans-serif` resolves to, for example to use an application font as
    /// the default. Existing paragraphs keep their fonts until restyled.
    pub fn set_generic_families(
        &mut self,
        _generic: GenericFamily,
        _families: impl IntoIterator<Item = FontFamily>,
    ) {
        let families: Vec<FontFamily> = _families.into_iter().collect();
        #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
        self.fonts
            .collection
            .set_generic_families(_generic.fontique(), families.iter().map(|f| f.0));
        #[cfg(not(any(feature = "text-parley", not(target_os = "linux"))))]
        self.backend.set_generic_families(_generic, &families);
    }

    /// Shapes text once and lays it out without soft wrapping.
    ///
    /// Inspect [`Paragraph::diagnostics`] for missing fonts or glyphs. The layout
    /// uses unquantized logical pixels; a renderer performs device-pixel snapping.
    pub fn paragraph(
        &mut self,
        text: impl Into<String>,
        style: &TextStyle<'_>,
    ) -> Result<Paragraph, TextError> {
        let text = text.into();
        validate_text(&text, style)?;
        let mut paragraph = Paragraph {
            text,
            layout: Layout::empty(),
            width: None,
            alignment: Alignment::Start,
            diagnostics: TextDiagnostics::default(),
            content_widths: ContentWidths { min: 0.0, max: 0.0 },
            shaping: self.shaping(style),
            color: style.color,
            released: false,
        };
        self.build(&mut paragraph);
        Ok(paragraph)
    }

    /// Replaces content and styling, reusing owned text and layout allocations.
    ///
    /// The current wrapping width and alignment are preserved. Invalid arguments
    /// return before modifying the paragraph. A released paragraph only stores
    /// both until [`Self::restore`].
    pub fn update(
        &mut self,
        paragraph: &mut Paragraph,
        text: &str,
        style: &TextStyle<'_>,
    ) -> Result<(), TextError> {
        validate_text(text, style)?;
        paragraph.text.clear();
        paragraph.text.push_str(text);
        paragraph.color = style.color;
        if !paragraph.shaping.matches(style) {
            paragraph.shaping = self.shaping(style);
        }
        if !paragraph.released {
            self.build(paragraph);
        }
        Ok(())
    }

    /// Applies a new style without copying the text. Only a change other than
    /// the color shapes again; a released paragraph stores the style until
    /// [`Self::restore`].
    pub fn restyle(
        &mut self,
        paragraph: &mut Paragraph,
        style: &TextStyle<'_>,
    ) -> Result<(), TextError> {
        style.validate()?;
        paragraph.color = style.color;
        if paragraph.shaping.matches(style) {
            return Ok(());
        }
        paragraph.shaping = self.shaping(style);
        if !paragraph.released {
            self.build(paragraph);
        }
        Ok(())
    }

    /// Shapes a paragraph [`Paragraph::release`] emptied again, with its
    /// retained text, style, width and alignment; others are unchanged.
    pub fn restore(&mut self, paragraph: &mut Paragraph) {
        if std::mem::take(&mut paragraph.released) {
            self.build(paragraph);
        }
    }

    /// Releases shaping scratch and cached file mappings held only by this system.
    ///
    /// Registered font blobs, font metadata and layouts are retained. Upstream
    /// shaping caches have a 16-entry limit but no public byte accounting; this
    /// method recreates scratch instead of pretending to enforce a byte budget.
    pub fn trim(&mut self) {
        self.backend.trim();
        #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
        {
            self.context = LayoutContext::new();
            self.fonts.source_cache.prune(0, true);
        }
        self.families
            .retain(|families| Arc::strong_count(families) > 1);
    }

    /// Limits the UTF-8 bytes of idle text, across every host sharing this
    /// system, that keeps its layout; `None`, the default, keeps all. Text is
    /// idle while its host does not show it. Hosts report their idle text with
    /// [`Self::report_idle`] and, while the total exceeds the budget, release
    /// their least recently shown paragraphs ([`Paragraph::release`]).
    /// Layout memory grows with the text, but this counts text bytes, not the
    /// layout's allocations.
    pub fn set_idle_budget(&mut self, bytes: impl Into<Option<usize>>) {
        self.idle_budget = bytes.into();
    }

    /// The budget of [`Self::set_idle_budget`].
    pub fn idle_budget(&self) -> Option<usize> {
        self.idle_budget
    }

    /// Idle text bytes still keeping their layouts, as hosts reported them.
    pub fn idle_bytes(&self) -> usize {
        self.idle_bytes
    }

    /// Replaces a host's earlier report of `previous` idle bytes with
    /// `current`; a host reports zero when it goes away.
    pub fn report_idle(&mut self, previous: usize, current: usize) {
        self.idle_bytes = self.idle_bytes - previous + current;
    }

    /// Whether idle text exceeds the budget.
    pub fn over_idle_budget(&self) -> bool {
        self.idle_budget
            .is_some_and(|budget| self.idle_bytes > budget)
    }

    /// The shaping key of `style`, sharing an equal family list already in use.
    pub(crate) fn shaping(&mut self, style: &TextStyle<'_>) -> Shaping {
        let families = match self.families.iter().find(|f| ***f == *style.families) {
            Some(families) => Arc::clone(families),
            None => {
                let families = Arc::<str>::from(style.families);
                self.families.push(Arc::clone(&families));
                families
            }
        };
        Shaping {
            families,
            size: style.size,
            weight: style.weight,
            width: style.width,
            slant: style.slant,
            locale: style.language(),
            line_height: style.line_height,
            letter_spacing: style.letter_spacing,
            word_spacing: style.word_spacing,
        }
    }

    pub(crate) fn shape_layout(
        &mut self,
        text: &str,
        shaping: &Shaping,
        wrap: bool,
        underline: Option<Range<usize>>,
    ) -> Layout {
        #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
        {
            let mut raw = parley::Layout::new();
            self.shape(&mut raw, text, shaping, Color::BLACK, underline, wrap);
            Layout::Parley(crate::backend_parley::ParleyLayout { raw })
        }
        #[cfg(not(any(feature = "text-parley", not(target_os = "linux"))))]
        {
            Layout::new(&mut self.backend, text, shaping, wrap, underline)
        }
    }

    #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
    pub(crate) fn shape(
        &mut self,
        layout: &mut parley::Layout<Color>,
        text: &str,
        shaping: &Shaping,
        color: Color,
        underline: Option<std::ops::Range<usize>>,
        wrap: bool,
    ) {
        let mut builder = self
            .context
            .ranged_builder(&mut self.fonts, text, 1.0, false);
        for property in shaping.properties(color) {
            builder.push_default(property);
        }
        builder.push_default(parley::StyleProperty::TextWrapMode(if wrap {
            parley::TextWrapMode::Wrap
        } else {
            parley::TextWrapMode::NoWrap
        }));
        if let Some(range) = underline.filter(|range| !range.is_empty()) {
            builder.push(parley::StyleProperty::Underline(true), range);
        }
        builder.build_into(layout, text);
    }

    fn build(&mut self, paragraph: &mut Paragraph) {
        let text = paragraph.text.clone();
        let shaping = paragraph.shaping.clone();
        paragraph.layout = self.shape_layout(&text, &shaping, true, None);
        paragraph.content_widths = paragraph.layout.content_widths();
        paragraph
            .layout
            .reflow(paragraph.width, paragraph.alignment);
        paragraph.update_diagnostics();
    }
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_text(text: &str, style: &TextStyle<'_>) -> Result<(), TextError> {
    if text.len() > u32::MAX as usize {
        return Err(TextError::TextTooLong);
    }
    style.validate()
}
