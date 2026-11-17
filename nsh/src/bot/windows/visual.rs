//! A browser visual stacked between NSH content and its later glass overlays.
use nsh_platform_windows::Placement;
use windows::{
    Win32::Graphics::DirectComposition::{
        IDCompositionDesktopDevice, IDCompositionRectangleClip, IDCompositionVisual2,
        IDCompositionVisual3,
    },
    core::{Interface, Result},
};

pub(super) struct Visual {
    pub outer: IDCompositionVisual2,
    pub target: IDCompositionVisual2,
    corners: IDCompositionRectangleClip,
}
impl Visual {
    pub fn new(device: &IDCompositionDesktopDevice) -> Result<Self> {
        // SAFETY: the device and all visuals live on NSH's platform thread.
        unsafe {
            let outer = device.CreateVisual()?;
            let target = device.CreateVisual()?;
            let corners = device.CreateRectangleClip()?;
            target.SetClip(&corners)?;
            outer.AddVisual(&target, true, None)?;
            Ok(Self {
                outer,
                target,
                corners,
            })
        }
    }
    pub fn place(&self, at: Option<Placement>) {
        let Some(p) = at else {
            return;
        };
        // NSH commits and removes invisible visuals after placement. Bounds and
        // clipping use the same contract as NSH's own WebView2 visual host.
        unsafe {
            let _ = self.target.SetTransform2(&p.matrix);
            let _ = self.corners.SetLeft2(0.0);
            let _ = self.corners.SetTop2(0.0);
            let _ = self.corners.SetRight2(p.size.0 as f32);
            let _ = self.corners.SetBottom2(p.size.1 as f32);
            let [tl, tr, br, bl] = p.radii;
            let _ = self.corners.SetTopLeftRadiusX2(tl);
            let _ = self.corners.SetTopLeftRadiusY2(tl);
            let _ = self.corners.SetTopRightRadiusX2(tr);
            let _ = self.corners.SetTopRightRadiusY2(tr);
            let _ = self.corners.SetBottomRightRadiusX2(br);
            let _ = self.corners.SetBottomRightRadiusY2(br);
            let _ = self.corners.SetBottomLeftRadiusX2(bl);
            let _ = self.corners.SetBottomLeftRadiusY2(bl);
            let _ = self.outer.SetClip2(&p.clip);
            if let Ok(outer) = self.outer.cast::<IDCompositionVisual3>() {
                let _ = outer.SetOpacity2(p.alpha.clamp(0.0, 1.0));
            }
        }
    }
}
