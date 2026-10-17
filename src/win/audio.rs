use super::sys::{post, WM_AUDIO, WM_DEVICES};
use crate::config::Target;
use windows::core::*;
use windows::Win32::Devices::FunctionDiscovery::*;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::*;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

/// Event context for our own SetMute calls, so callbacks can tell them apart.
const CTX: GUID = GUID::from_u128(0x6b1f3c2e_9a4d_4e1b_8c7a_2f5d9e8a1c33);

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCb;

impl IAudioEndpointVolumeCallback_Impl for VolumeCb_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        let ours = unsafe { data.as_ref().map(|d| d.guidEventContext == CTX).unwrap_or(false) };
        post(WM_AUDIO, if ours { 0 } else { 1 }, 0);
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceCb;

impl IMMNotificationClient_Impl for DeviceCb_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        post(WM_DEVICES, 0, 0);
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        post(WM_DEVICES, 0, 0);
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        post(WM_DEVICES, 0, 0);
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, _: ERole, _: &PCWSTR) -> Result<()> {
        if flow == eCapture {
            post(WM_DEVICES, 0, 0);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

pub struct Device {
    pub id: String,
    pub name: String,
    pub adapter: String,
}

struct Endpoint {
    vol: IAudioEndpointVolume,
    cb: IAudioEndpointVolumeCallback,
}

pub struct Audio {
    en: IMMDeviceEnumerator,
    dev_cb: IMMNotificationClient,
    eps: Vec<Endpoint>,
    pub devices: Vec<Device>,
    pub default_id: Option<String>,
}

fn prop(store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore, key: &PROPERTYKEY) -> String {
    unsafe {
        let Ok(v) = store.GetValue(key) else { return String::new() };
        let inner = &v.Anonymous.Anonymous;
        if inner.vt == windows::Win32::System::Variant::VT_LPWSTR && !inner.Anonymous.pwszVal.is_null() {
            inner.Anonymous.pwszVal.to_string().unwrap_or_default()
        } else {
            String::new()
        }
    }
}

fn device_id(d: &IMMDevice) -> Option<String> {
    unsafe {
        let p = d.GetId().ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s
    }
}

impl Audio {
    pub fn new() -> Result<Self> {
        unsafe {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let dev_cb: IMMNotificationClient = DeviceCb.into();
            en.RegisterEndpointNotificationCallback(&dev_cb)?;
            Ok(Self { en, dev_cb, eps: Vec::new(), devices: Vec::new(), default_id: None })
        }
    }

    fn release(&mut self) {
        for ep in self.eps.drain(..) {
            unsafe {
                let _ = ep.vol.UnregisterControlChangeNotify(&ep.cb);
            }
        }
    }

    pub fn refresh(&mut self, target: &Target) {
        self.release();
        self.devices.clear();
        self.default_id = None;
        unsafe {
            let mut found: Vec<(String, IMMDevice)> = Vec::new();
            if let Ok(coll) = self.en.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) {
                let n = coll.GetCount().unwrap_or(0);
                for i in 0..n {
                    let Ok(d) = coll.Item(i) else { continue };
                    let Some(id) = device_id(&d) else { continue };
                    let (name, adapter) = match d.OpenPropertyStore(STGM_READ) {
                        Ok(st) => (prop(&st, &PKEY_Device_DeviceDesc), prop(&st, &PKEY_DeviceInterface_FriendlyName)),
                        Err(_) => (String::new(), String::new()),
                    };
                    self.devices.push(Device { id: id.clone(), name, adapter });
                    found.push((id, d));
                }
            }
            self.default_id = self.en.GetDefaultAudioEndpoint(eCapture, eConsole).ok().and_then(|d| device_id(&d));

            let pick: Vec<&IMMDevice> = match target {
                Target::All => found.iter().map(|(_, d)| d).collect(),
                Target::Default => found.iter().filter(|(id, _)| Some(id) == self.default_id.as_ref()).map(|(_, d)| d).collect(),
                Target::Device(want) => found.iter().filter(|(id, _)| id == want).map(|(_, d)| d).collect(),
            };
            for d in pick {
                let Ok(vol) = d.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) else { continue };
                let cb: IAudioEndpointVolumeCallback = VolumeCb.into();
                let _ = vol.RegisterControlChangeNotify(&cb);
                self.eps.push(Endpoint { vol, cb });
            }
        }
    }

    /// None when there is nothing to control.
    pub fn muted(&self) -> Option<bool> {
        if self.eps.is_empty() {
            return None;
        }
        let all = self.eps.iter().all(|e| unsafe { e.vol.GetMute().map(|b| b.as_bool()).unwrap_or(false) });
        Some(all)
    }

    pub fn set_muted(&self, m: bool) {
        for e in &self.eps {
            unsafe {
                let cur = e.vol.GetMute().map(|b| b.as_bool()).unwrap_or(!m);
                if cur != m {
                    let _ = e.vol.SetMute(m, &CTX);
                }
            }
        }
    }

    pub fn healthy(&self) -> bool {
        self.eps.iter().all(|e| unsafe { e.vol.GetMute().is_ok() })
    }

    pub fn name_of(&self, id: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.id == id)
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.release();
        unsafe {
            let _ = self.en.UnregisterEndpointNotificationCallback(&self.dev_cb);
        }
    }
}
