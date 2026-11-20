//! User-visible and accessible strings in Korean and English.

use windowsrice_core::config::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Ko,
}

impl Lang {
    /// Resolves the configured language. `system_is_korean` reflects the
    /// Windows display language and is used for [`Language::Auto`].
    pub fn resolve(configured: Language, system_is_korean: bool) -> Self {
        match configured {
            Language::English => Lang::En,
            Language::Korean => Lang::Ko,
            Language::Auto if system_is_korean => Lang::Ko,
            Language::Auto => Lang::En,
        }
    }

    /// BCP 47 tag for DirectWrite locale-sensitive shaping.
    pub fn locale(self) -> &'static str {
        match self {
            Lang::En => "en-US",
            Lang::Ko => "ko-KR",
        }
    }
}

/// Fixed strings. Parameterised strings are methods.
#[derive(Debug, Clone, Copy)]
pub struct Strings {
    pub lang: Lang,
}

impl Strings {
    pub fn new(lang: Lang) -> Self {
        Self { lang }
    }

    fn pick(&self, en: &'static str, ko: &'static str) -> &'static str {
        match self.lang {
            Lang::En => en,
            Lang::Ko => ko,
        }
    }

    pub fn panel(&self) -> &'static str {
        self.pick("windowsRICE panel", "windowsRICE 패널")
    }
    pub fn group_app(&self) -> &'static str {
        self.pick("Application", "응용 프로그램")
    }
    pub fn group_clock(&self) -> &'static str {
        self.pick("Clock", "시계")
    }
    pub fn group_status(&self) -> &'static str {
        self.pick("System status", "시스템 상태")
    }
    pub fn menu_button(&self) -> &'static str {
        self.pick("windowsRICE menu", "windowsRICE 메뉴")
    }
    pub fn menu_help(&self) -> &'static str {
        self.pick(
            "Opens the windowsRICE menu. Press Enter, Space or Down Arrow.",
            "windowsRICE 메뉴를 엽니다. Enter, Space 또는 아래쪽 화살표를 누르세요.",
        )
    }
    pub fn desktop(&self) -> &'static str {
        self.pick("Desktop", "바탕 화면")
    }

    pub fn active_window(&self, title: &str) -> String {
        match self.lang {
            Lang::En => format!("Active window: {title}"),
            Lang::Ko => format!("활성 창: {title}"),
        }
    }

    pub fn network(&self, state: NetworkPhrase) -> &'static str {
        use NetworkPhrase as N;
        match (self.lang, state) {
            (Lang::En, N::InternetWired) => "Network: connected to the internet, wired",
            (Lang::En, N::InternetWireless) => "Network: connected to the internet, Wi-Fi",
            (Lang::En, N::Internet) => "Network: connected to the internet",
            (Lang::En, N::LocalOnly) => "Network: connected, no internet access",
            (Lang::En, N::Disconnected) => "Network: disconnected",
            (Lang::En, N::Unavailable) => "Network status unavailable",
            (Lang::Ko, N::InternetWired) => "네트워크: 인터넷 연결됨, 유선",
            (Lang::Ko, N::InternetWireless) => "네트워크: 인터넷 연결됨, Wi-Fi",
            (Lang::Ko, N::Internet) => "네트워크: 인터넷 연결됨",
            (Lang::Ko, N::LocalOnly) => "네트워크: 연결됨, 인터넷 없음",
            (Lang::Ko, N::Disconnected) => "네트워크: 연결 끊김",
            (Lang::Ko, N::Unavailable) => "네트워크 상태를 알 수 없음",
        }
    }

    pub fn volume(&self, percent: u8, muted: bool) -> String {
        match (self.lang, muted) {
            (Lang::En, false) => format!("Volume {percent}%"),
            (Lang::En, true) => format!("Volume muted ({percent}%)"),
            (Lang::Ko, false) => format!("볼륨 {percent}%"),
            (Lang::Ko, true) => format!("음소거 (볼륨 {percent}%)"),
        }
    }
    pub fn volume_unavailable(&self) -> &'static str {
        self.pick("No audio output device", "오디오 출력 장치 없음")
    }

    pub fn cpu(&self, percent: u8) -> String {
        match self.lang {
            Lang::En => format!("CPU usage {percent}%"),
            Lang::Ko => format!("CPU 사용률 {percent}%"),
        }
    }
    pub fn cpu_unavailable(&self) -> &'static str {
        self.pick("CPU usage unavailable", "CPU 사용률을 알 수 없음")
    }

    pub fn gpu(&self, percent: u8) -> String {
        match self.lang {
            Lang::En => format!("GPU usage {percent}%"),
            Lang::Ko => format!("GPU 사용률 {percent}%"),
        }
    }
    pub fn gpu_unavailable(&self) -> &'static str {
        self.pick("GPU usage unavailable", "GPU 사용률을 알 수 없음")
    }

    pub fn memory(&self, used: &str, total: &str, percent: u8) -> String {
        match self.lang {
            Lang::En => format!("Memory: {used} of {total} used ({percent}%)"),
            Lang::Ko => format!("메모리: {total} 중 {used} 사용 ({percent}%)"),
        }
    }
    pub fn memory_unavailable(&self) -> &'static str {
        self.pick("Memory usage unavailable", "메모리 사용량을 알 수 없음")
    }

    pub fn status_ok(&self) -> &'static str {
        self.pick("windowsRICE status: normal", "windowsRICE 상태: 정상")
    }
    pub fn status_warnings(&self, warnings: &[String]) -> String {
        let list = warnings.join("; ");
        match self.lang {
            Lang::En => format!("windowsRICE status: {} warning(s): {list}", warnings.len()),
            Lang::Ko => format!("windowsRICE 상태: 경고 {}개: {list}", warnings.len()),
        }
    }

    pub fn menu_exit(&self) -> &'static str {
        self.pick("Exit panel", "패널 종료")
    }
    pub fn menu_about(&self, version: &str, theme: &str) -> String {
        match self.lang {
            Lang::En => format!("windowsRICE {version} — theme: {theme}"),
            Lang::Ko => format!("windowsRICE {version} — 테마: {theme}"),
        }
    }
}

/// Network states with distinct wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPhrase {
    InternetWired,
    InternetWireless,
    Internet,
    LocalOnly,
    Disconnected,
    Unavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_system() {
        assert_eq!(Lang::resolve(Language::Auto, true), Lang::Ko);
        assert_eq!(Lang::resolve(Language::Auto, false), Lang::En);
        assert_eq!(Lang::resolve(Language::English, true), Lang::En);
        assert_eq!(Lang::resolve(Language::Korean, false), Lang::Ko);
    }
}
