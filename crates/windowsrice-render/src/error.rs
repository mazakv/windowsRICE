use windows::core::HRESULT;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// The graphics device was removed or reset (driver update, TDR, GPU
    /// switch). Call [`crate::Renderer::recover`].
    #[error("graphics device lost: {0}")]
    DeviceLost(String),
    #[error(transparent)]
    Windows(#[from] windows::core::Error),
}

const D2DERR_RECREATE_TARGET: HRESULT = HRESULT(0x8899_000C_u32.cast_signed());
const DXGI_ERROR_DEVICE_REMOVED: HRESULT = HRESULT(0x887A_0005_u32.cast_signed());
const DXGI_ERROR_DEVICE_RESET: HRESULT = HRESULT(0x887A_0007_u32.cast_signed());
const DXGI_ERROR_DEVICE_HUNG: HRESULT = HRESULT(0x887A_0006_u32.cast_signed());
const DXGI_ERROR_DRIVER_INTERNAL_ERROR: HRESULT = HRESULT(0x887A_0020_u32.cast_signed());

/// Classifies a Windows error as device loss where appropriate.
pub(crate) fn classify(e: windows::core::Error) -> RenderError {
    let c = e.code();
    if c == D2DERR_RECREATE_TARGET
        || c == DXGI_ERROR_DEVICE_REMOVED
        || c == DXGI_ERROR_DEVICE_RESET
        || c == DXGI_ERROR_DEVICE_HUNG
        || c == DXGI_ERROR_DRIVER_INTERNAL_ERROR
    {
        RenderError::DeviceLost(format!("0x{:08X}", c.0.cast_unsigned()))
    } else {
        RenderError::Windows(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_loss_codes_are_classified() {
        for code in [0x8899_000Cu32, 0x887A_0005, 0x887A_0007] {
            let e = windows::core::Error::from(HRESULT(code.cast_signed()));
            assert!(
                matches!(classify(e), RenderError::DeviceLost(_)),
                "{code:08X}"
            );
        }
        let e = windows::core::Error::from(HRESULT(0x8007_000Eu32.cast_signed()));
        assert!(matches!(classify(e), RenderError::Windows(_)));
    }
}
