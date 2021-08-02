//! Images.
//! This module has some helpers for modeling terminal cells that are filled
//! with image data.
//! We're targeting the iTerm image protocol initially, with sixel as an obvious
//! follow up.
//! Kitty has an extensive and complex graphics protocol
//! whose docs are here:
//! <https://github.com/kovidgoyal/kitty/blob/master/docs/graphics-protocol.rst>
//! Both iTerm2 and Sixel appear to have semantics that allow replacing the
//! contents of a single chararcter cell with image data, whereas the kitty
//! protocol appears to track the images out of band as attachments with
//! z-order.

use ordered_float::NotNan;
#[cfg(feature = "use_serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "use_serde")]
fn deserialize_notnan<'de, D>(deserializer: D) -> Result<NotNan<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    NotNan::new(value).map_err(|e| serde::de::Error::custom(format!("{:?}", e)))
}

#[cfg(feature = "use_serde")]
#[cfg_attr(feature = "cargo-clippy", allow(clippy::trivially_copy_pass_by_ref))]
fn serialize_notnan<S>(value: &NotNan<f32>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value.into_inner().serialize(serializer)
}

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureCoordinate {
    #[cfg_attr(
        feature = "use_serde",
        serde(
            deserialize_with = "deserialize_notnan",
            serialize_with = "serialize_notnan"
        )
    )]
    pub x: NotNan<f32>,
    #[cfg_attr(
        feature = "use_serde",
        serde(
            deserialize_with = "deserialize_notnan",
            serialize_with = "serialize_notnan"
        )
    )]
    pub y: NotNan<f32>,
}

impl TextureCoordinate {
    pub fn new(x: NotNan<f32>, y: NotNan<f32>) -> Self {
        Self { x, y }
    }

    pub fn new_f32(x: f32, y: f32) -> Self {
        let x = NotNan::new(x).unwrap();
        let y = NotNan::new(y).unwrap();
        Self::new(x, y)
    }
}

/// Tracks data for displaying an image in the place of the normal cell
/// character data.  Since an Image can span multiple cells, we need to logically
/// carve up the image and track each slice of it.  Each cell needs to know
/// its "texture coordinates" within that image so that we can render the
/// right slice.
#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageCell {
    /// Texture coordinate for the top left of this cell.
    /// (0,0) is the top left of the ImageData. (1, 1) is
    /// the bottom right.
    top_left: TextureCoordinate,
    /// Texture coordinates for the bottom right of this cell.
    bottom_right: TextureCoordinate,
    /// References the underlying image data
    data: Arc<ImageData>,
    z_index: i32,
    /// When rendering in the cell, use this offset from the top left
    /// of the cell
    display_offset_x: u32,
    display_offset_y: u32,

    image_id: u32,
    placement_id: Option<u32>,
}

impl ImageCell {
    pub fn new(
        top_left: TextureCoordinate,
        bottom_right: TextureCoordinate,
        data: Arc<ImageData>,
    ) -> Self {
        Self::with_z_index(top_left, bottom_right, data, 0, 0, 0, 0, None)
    }

    pub fn with_z_index(
        top_left: TextureCoordinate,
        bottom_right: TextureCoordinate,
        data: Arc<ImageData>,
        z_index: i32,
        display_offset_x: u32,
        display_offset_y: u32,
        image_id: u32,
        placement_id: Option<u32>,
    ) -> Self {
        Self {
            top_left,
            bottom_right,
            data,
            z_index,
            display_offset_x,
            display_offset_y,
            image_id,
            placement_id,
        }
    }

    pub fn matches_placement(&self, image_id: u32, placement_id: Option<u32>) -> bool {
        self.image_id == image_id && self.placement_id == placement_id
    }

    pub fn top_left(&self) -> TextureCoordinate {
        self.top_left
    }

    pub fn bottom_right(&self) -> TextureCoordinate {
        self.bottom_right
    }

    pub fn image_data(&self) -> &Arc<ImageData> {
        &self.data
    }

    /// negative z_index is rendered beneath the text layer.
    /// >= 0 is rendered above the text.
    /// negative z_index < INT32_MIN/2 will be drawn under cells
    /// with non-default background colors
    pub fn z_index(&self) -> i32 {
        self.z_index
    }

    pub fn display_offset(&self) -> (u32, u32) {
        (self.display_offset_x, self.display_offset_y)
    }
}

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Clone, PartialEq, Eq)]
pub enum ImageDataType {
    /// Data is in the native image file format
    /// (best for file formats that have animated content)
    EncodedFile(Vec<u8>),
    /// Data is RGBA u8 data
    Rgba8 {
        data: Vec<u8>,
        width: u32,
        height: u32,
    },
    /// Data is an animated sequence
    AnimRgba8 {
        width: u32,
        height: u32,
        durations: Vec<Duration>,
        frames: Vec<Vec<u8>>,
    },
}

impl std::fmt::Debug for ImageDataType {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::EncodedFile(data) => fmt
                .debug_struct("EncodedFile")
                .field("data_of_len", &data.len())
                .finish(),
            Self::Rgba8 {
                data,
                width,
                height,
            } => fmt
                .debug_struct("Rgba8")
                .field("data_of_len", &data.len())
                .field("width", &width)
                .field("height", &height)
                .finish(),
            Self::AnimRgba8 {
                frames,
                width,
                height,
                durations,
            } => fmt
                .debug_struct("AnimRgba8")
                .field("frames_of_len", &frames.len())
                .field("width", &width)
                .field("height", &height)
                .field("durations", durations)
                .finish(),
        }
    }
}

impl ImageDataType {
    pub fn compute_hash(&self) -> [u8; 32] {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        match self {
            ImageDataType::EncodedFile(data) => hasher.update(data),
            ImageDataType::Rgba8 { data, .. } => hasher.update(data),
            ImageDataType::AnimRgba8 { frames, .. } => {
                for data in frames {
                    hasher.update(data);
                }
            }
        };
        hasher.finalize().into()
    }

    /// Decode an encoded file into either an Rgba8 or AnimRgba8 variant
    /// if we recognize the file format, otherwise the EncodedFile data
    /// is preserved as is.
    #[cfg(feature = "use_image")]
    pub fn decode(self) -> Self {
        use image::{AnimationDecoder, ImageFormat};

        match self {
            Self::EncodedFile(data) => {
                let format = match image::guess_format(&data) {
                    Ok(format) => format,
                    _ => return Self::EncodedFile(data),
                };
                match format {
                    ImageFormat::Gif => image::gif::GifDecoder::new(&*data)
                        .and_then(|decoder| decoder.into_frames().collect_frames())
                        .and_then(|frames| Ok(Self::decode_frames(frames)))
                        .unwrap_or_else(|err| {
                            log::error!(
                                "Unable to parse animated gif: {:#}, trying as single frame",
                                err
                            );
                            Self::decode_single(data)
                        }),
                    ImageFormat::Png => {
                        let decoder = match image::png::PngDecoder::new(&*data) {
                            Ok(d) => d,
                            _ => return Self::EncodedFile(data),
                        };
                        if decoder.is_apng() {
                            match decoder.apng().into_frames().collect_frames() {
                                Ok(frames) => Self::decode_frames(frames),
                                _ => Self::EncodedFile(data),
                            }
                        } else {
                            Self::decode_single(data)
                        }
                    }
                    _ => Self::EncodedFile(data),
                }
            }
            data => data,
        }
    }

    #[cfg(feature = "use_image")]
    fn decode_frames(img_frames: Vec<image::Frame>) -> Self {
        let mut width = 0;
        let mut height = 0;
        let mut frames = vec![];
        let mut durations = vec![];
        for frame in img_frames.into_iter() {
            let duration: Duration = frame.delay().into();
            durations.push(duration);
            let image = image::DynamicImage::ImageRgba8(frame.into_buffer()).to_rgba8();
            let (w, h) = image.dimensions();
            width = w;
            height = h;
            frames.push(image.into_vec());
        }
        Self::AnimRgba8 {
            width,
            height,
            frames,
            durations,
        }
    }

    #[cfg(feature = "use_image")]
    fn decode_single(data: Vec<u8>) -> Self {
        match image::load_from_memory(&data) {
            Ok(image) => {
                let image = image.to_rgba8();
                let (width, height) = image.dimensions();
                Self::Rgba8 {
                    width,
                    height,
                    data: image.into_vec(),
                }
            }
            _ => Self::EncodedFile(data),
        }
    }
}

static IMAGE_ID: ::std::sync::atomic::AtomicUsize = ::std::sync::atomic::AtomicUsize::new(0);

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageData {
    id: usize,
    hash: [u8; 32],
    data: ImageDataType,
}

impl ImageData {
    /// Create a new ImageData struct with the provided raw data.
    pub fn with_raw_data(data: Vec<u8>) -> Self {
        Self::with_data(ImageDataType::EncodedFile(data))
    }

    pub fn with_data(data: ImageDataType) -> Self {
        let id = IMAGE_ID.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed);
        let hash = data.compute_hash();
        Self { id, hash, data }
    }

    /// Returns the in-memory footprint
    pub fn len(&self) -> usize {
        match &self.data {
            ImageDataType::EncodedFile(d) => d.len(),
            ImageDataType::Rgba8 { data, .. } => data.len(),
            ImageDataType::AnimRgba8 { frames, .. } => frames.len() * frames[0].len(),
        }
    }

    #[inline]
    pub fn data(&self) -> &ImageDataType {
        &self.data
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }
}
