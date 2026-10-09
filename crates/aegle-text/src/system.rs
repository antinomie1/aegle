use crate::{
    Alignment, Blob, Collection, ContentWidths, FamilyId, FontInfo, Paragraph, TextDiagnostics,
    TextError, TextStyle,
};
use aegle_types::Color;
use parley::fontique::{CollectionOptions, SourceCache};
use parley::{FontContext, LayoutContext, StyleProperty};

/// Font collection and reusable shaping scratch for one UI thread.
///
/// Context scratch grows to accommodate shaped text. Call [`Self::trim`] under
/// memory pressure to release it. Font metadata and fonts referenced by live
/// paragraphs remain owned; this is not a byte-budgeted glyph bitmap cache.
pub struct TextSystem {
    pub(crate) fonts: FontContext,
    pub(crate) context: LayoutContext<Color>,
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

    fn with_system_fonts(system_fonts: bool) -> Self {
        Self {
            fonts: FontContext {
                collection: Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts,
                }),
                source_cache: SourceCache::default(),
            },
            context: LayoutContext::new(),
        }
    }

    /// Registers all readable faces from shared font bytes, without copying them.
    ///
    /// Returns family IDs and registered face information. Supplying the same
    /// [`Blob`] to a rasterizer preserves its resource identity. Unsupported or
    /// invalid faces are ignored by Fontique; zero registered faces is an error.
    pub fn register_fonts(
        &mut self,
        data: Blob<u8>,
    ) -> Result<Vec<(FamilyId, Vec<FontInfo>)>, TextError> {
        let registered = self.fonts.collection.register_fonts(data, None);
        if registered.is_empty() {
            return Err(TextError::InvalidFont);
        }
        Ok(registered)
    }

    /// Accesses family metadata and explicit generic/script/locale fallback maps.
    ///
    /// For example, use [`Collection::set_generic_families`] to associate an
    /// application font with `sans-serif`. Collection changes do not rebuild
    /// existing paragraphs; call [`Self::restyle`] after changing their fonts.
    pub fn collection_mut(&mut self) -> &mut Collection {
        &mut self.fonts.collection
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
            layout: Default::default(),
            width: None,
            alignment: Alignment::Start,
            diagnostics: TextDiagnostics::default(),
            content_widths: ContentWidths { min: 0.0, max: 0.0 },
            weight: style.weight.value(),
        };
        self.build(&mut paragraph, style);
        Ok(paragraph)
    }

    /// Replaces content and styling, reusing owned text and layout allocations.
    ///
    /// The current wrapping width and alignment are preserved. Invalid arguments
    /// return before modifying the paragraph.
    pub fn update(
        &mut self,
        paragraph: &mut Paragraph,
        text: &str,
        style: &TextStyle<'_>,
    ) -> Result<(), TextError> {
        validate_text(text, style)?;
        paragraph.text.clear();
        paragraph.text.push_str(text);
        self.build(paragraph, style);
        Ok(())
    }

    /// Rebuilds shaping after font/style changes without copying the text.
    pub fn restyle(
        &mut self,
        paragraph: &mut Paragraph,
        style: &TextStyle<'_>,
    ) -> Result<(), TextError> {
        style.validate()?;
        self.build(paragraph, style);
        Ok(())
    }

    /// Releases shaping scratch and cached file mappings held only by this system.
    ///
    /// Registered font blobs, font metadata and layouts are retained. Upstream
    /// shaping caches have a 16-entry limit but no public byte accounting; this
    /// method recreates scratch instead of pretending to enforce a byte budget.
    pub fn trim(&mut self) {
        self.context = LayoutContext::new();
        self.fonts.source_cache.prune(0, true);
    }

    fn build(&mut self, paragraph: &mut Paragraph, style: &TextStyle<'_>) {
        let mut builder = self
            .context
            .ranged_builder(&mut self.fonts, &paragraph.text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(style.families.into()));
        for property in style.common_properties() {
            builder.push_default(property);
        }
        builder.build_into(&mut paragraph.layout, &paragraph.text);
        paragraph.weight = style.weight.value();
        paragraph.content_widths = paragraph.layout.calculate_content_widths();
        paragraph.break_lines();
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
