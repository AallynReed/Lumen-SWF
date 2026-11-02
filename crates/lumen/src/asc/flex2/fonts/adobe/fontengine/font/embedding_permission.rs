//! Port of afe.jar `com.adobe.fontengine.font.EmbeddingPermission`.

use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;

const XUID_EMBED_OK_TABLE_ALL: [(i32, i32, i32); 202] = [
    (1, 11, 1800000), (1, 11, 1802000), (1, 11, 1803100), (1, 11, 1804200), (1, 11, 1805300), (1, 11, 1806400), (1, 11, 1807500),
    (1, 11, 1808600), (1, 11, 1809700), (1, 11, 1810800), (1, 11, 1811900), (1, 11, 1813000), (1, 11, 1814100), (1, 11, 1815200),
    (1, 11, 1816300), (1, 11, 1817400), (1, 11, 1818500), (1, 11, 1819600), (1, 11, 1820700), (1, 11, 1821800), (1, 11, 1822900),
    (1, 11, 1824000), (1, 11, 1825100), (1, 11, 1826200), (1, 11, 1827300), (1, 11, 1828500), (1, 11, 1829600), (1, 11, 1830700),
    (1, 11, 1837300), (1, 11, 1838400), (1, 11, 2219600), (1, 11, 2254000), (1, 11, 2279000), (1, 11, 2279400), (1, 11, 2285000),
    (1, 11, 2294900), (1, 11, 2296000), (1, 11, 2297100), (1, 11, 2298200), (1, 11, 2400000), (1, 11, 2401100), (1, 11, 2402200),
    (1, 11, 2403300), (1, 11, 2418700), (1, 11, 2419800), (1, 11, 2420900), (1, 11, 2422000), (1, 11, 2423100), (1, 11, 2424200),
    (1, 11, 2425300), (1, 11, 2426400), (1, 11, 2427500), (1, 11, 2428600), (1, 11, 2429700), (1, 11, 2430800), (1, 11, 2431900),
    (1, 11, 2433000), (1, 11, 2434100), (1, 11, 2446600), (1, 11, 2447700), (1, 11, 2448800), (1, 11, 2449900), (1, 11, 2451000),
    (1, 11, 2452100), (1, 11, 2453200), (1, 11, 2454300), (1, 11, 2455400), (1, 11, 2456500), (1, 11, 2457600), (1, 11, 2458700),
    (1, 11, 2459800), (1, 11, 2460900), (1, 11, 2461000), (1, 11, 2462100), (1, 11, 2464300), (1, 11, 2465100), (1, 11, 2465900),
    (1, 11, 2466700), (1, 11, 2467500), (1, 11, 2468600), (1, 11, 2471900), (1, 11, 2473000), (1, 11, 2474100), (1, 11, 2475200),
    (1, 11, 2476300), (1, 11, 2477400), (1, 11, 2478500), (1, 11, 2479600), (1, 11, 2480700), (1, 11, 2481800), (1, 11, 2487900),
    (1, 11, 2489000), (1, 11, 2490100), (1, 11, 2491200), (1, 11, 2492300), (1, 11, 2493400), (1, 11, 2494100), (1, 11, 2494800),
    (1, 11, 2495500), (1, 11, 2496200), (1, 11, 2496900), (1, 11, 2497600), (1, 11, 2498300), (1, 11, 2499000), (203, 1, 100),
    (203, 1, 200), (203, 1, 300), (203, 1, 400), (203, 1, 500), (207, 1104, 3033145), (207, 1108, 3136975), (207, 1109, 3138475),
    (207, 1111, 3034645), (207, 1203, 3167015), (207, 1204, 3036145), (207, 1206, 3139975), (207, 1207, 3037645), (207, 1209, 3141475),
    (207, 1211, 3039145), (207, 1303, 3168515), (207, 1311, 3271387), (207, 1504, 3256387), (207, 1511, 3263887), (207, 2104, 3040645),
    (207, 2108, 3124975), (207, 2109, 3126475), (207, 2111, 3042145), (207, 2203, 3164015), (207, 2204, 3043645), (207, 2206, 3127975),
    (207, 2207, 3045145), (207, 2209, 3129475), (207, 2211, 3046645), (207, 2303, 3165515), (207, 2311, 3272887), (207, 2504, 3257887),
    (207, 2511, 3265387), (207, 3104, 3048145), (207, 3108, 3142975), (207, 3109, 3144475), (207, 3111, 3049645), (207, 3203, 3170015),
    (207, 3204, 3051145), (207, 3206, 3145975), (207, 3207, 3052645), (207, 3209, 3147475), (207, 3211, 3054145), (207, 3303, 3171515),
    (207, 3311, 3274387), (207, 3504, 3259387), (207, 3511, 3266887), (207, 4104, 3055645), (207, 4108, 3130975), (207, 4109, 3132475),
    (207, 4111, 3057145), (207, 4203, 3173015), (207, 4204, 3058645), (207, 4206, 3133975), (207, 4207, 3060145), (207, 4209, 3135475),
    (207, 4211, 3061645), (207, 4311, 3275887), (207, 4504, 3260887), (207, 4511, 3268387), (207, 5104, 3063145), (207, 5108, 3148975),
    (207, 5109, 3150475), (207, 5111, 3064645), (207, 5203, 3176015), (207, 5204, 3066145), (207, 5206, 3151975), (207, 5207, 3067645),
    (207, 5209, 3153475), (207, 5211, 3069145), (207, 5303, 3177515), (207, 5311, 3277387), (207, 5504, 3262387), (207, 5511, 3269887),
    (207, 10025, 3290537), (207, 10026, 3292037), (207, 10027, 3293537), (207, 10028, 3295037), (207, 10029, 3296537),
    (207, 10033, 3307037), (207, 10055, 3308537), (207, 10077, 3310037), (207, 10078, 3311537), (207, 10099, 3313037),
    (207, 10210, 3298037), (207, 10211, 3299537), (207, 10312, 3301037), (207, 10314, 3302537), (207, 10315, 3304037),
    (207, 10319, 3305537), (207, 10910, 3314537), (207, 11010, 3316037), (207, 11111, 3317537), (207, 11313, 3319037),
    (207, 11414, 3320537), (207, 11515, 3322037), (207, 11717, 3323537), (207, 12020, 3325037),
];

const PUBLIC_DOMAIN_FONT_NAMES: [&str; 98] = [
    "BauerBodoni-Black", "BauerBodoni-BlackCond", "BauerBodoni-BlackItalic", "BauerBodoni-Bold", "BauerBodoni-BoldCond",
    "BauerBodoni-BoldItalic", "BauerBodoni-BoldItalicOsF", "BauerBodoni-BoldOsF", "BauerBodoni-Italic", "BauerBodoni-ItalicOsF",
    "BauerBodoni-Roman", "BauerBodoni-RomanSC", "BellCentennial-Address", "BellCentennial-BoldListing", "BellCentennial-BoldListingAlt",
    "BellCentennial-NameAndNumber", "BellCentennial-SubCaption", "BrushScript", "Candida-Bold", "Candida-Italic", "Candida-Roman",
    "CenturyOldStyle-Bold", "CenturyOldStyle-Italic", "CenturyOldStyle-Regular", "Charme", "Courier", "Courier-Bold",
    "Courier-BoldOblique", "Courier-Oblique", "Folio-Bold", "Folio-BoldCondensed", "Folio-ExtraBold", "Folio-Light", "Folio-Medium",
    "Futura", "Futura-Bold", "Futura-BoldOblique", "Futura-Book", "Futura-BookOblique", "Futura-CondExtraBoldObl", "Futura-Condensed",
    "Futura-CondensedBold", "Futura-CondensedBoldOblique", "Futura-CondensedExtraBold", "Futura-CondensedLight",
    "Futura-CondensedLightOblique", "Futura-CondensedOblique", "Futura-ExtraBold", "Futura-ExtraBoldOblique", "Futura-Heavy",
    "Futura-HeavyOblique", "Futura-Light", "Futura-LightOblique", "Futura-Oblique", "Hobo", "Impressum-Bold", "Impressum-Italic",
    "Impressum-Roman", "LetterGothic", "LetterGothic-Bold", "LetterGothic-BoldSlanted", "LetterGothic-Slanted", "NewsGothic",
    "NewsGothic-Bold", "NewsGothic-BoldOblique", "NewsGothic-Oblique", "OCRA", "OCRB", "Orator", "Orator-Slanted", "PrestigeElite",
    "PrestigeElite-Bold", "PrestigeElite-BoldSlanted", "PrestigeElite-Slanted", "Serifa-Black", "Serifa-Bold", "Serifa-Italic",
    "Serifa-Light", "Serifa-LightItalic", "Serifa-Roman", "SimonciniGaramond", "SimonciniGaramond-Bold", "SimonciniGaramond-Italic",
    "StempelSchneidler-Black", "StempelSchneidler-BlackItalic", "StempelSchneidler-Bold", "StempelSchneidler-BoldItalic",
    "StempelSchneidler-Italic", "StempelSchneidler-Light", "StempelSchneidler-LightItalic", "StempelSchneidler-MedItalic",
    "StempelSchneidler-Medium", "StempelSchneidler-Roman", "Stencil", "Weiss", "Weiss-Bold", "Weiss-ExtraBold", "Weiss-Italic",
];

pub struct EmbeddingPermission;

impl EmbeddingPermission {
    pub fn interpret_fs_type(n: i32) -> Permission {
        if n == 2 {
            return Permission::Restricted;
        }
        if n & 0x200 != 0 {
            return Permission::Restricted;
        }
        if n & 8 != 0 {
            return Permission::Editable;
        }
        if n & 4 != 0 {
            return Permission::PreviewAndPrint;
        }
        if n == 0 {
            return Permission::Installable;
        }
        Permission::IllegalValue
    }

    fn get_comparison_font_name(s: &str) -> String {
        let u: Vec<u16> = s.encode_utf16().collect();
        if u.len() > 7 && u[6] == '+' as u16 && u[..6].iter().all(|&c| (b'A' as u16..=b'Z' as u16).contains(&c)) {
            return String::from_utf16_lossy(&u[7..]);
        }
        s.to_string()
    }

    /// `Pattern.compile(p, CASE_INSENSITIVE).matcher(s).find()` for the two ASCII patterns.
    fn find_ci(s: &str, trademark: bool) -> bool {
        let l = s.to_ascii_lowercase();
        if !trademark {
            return l.contains("adobe");
        }
        let b = l.as_bytes();
        let ws = |c: u8| matches!(c, 0x20 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D);
        for start in 0..b.len() {
            if !b[start..].starts_with(b"trademark") {
                continue;
            }
            let mut j = start + 9;
            let k = j;
            while j < b.len() && ws(b[j]) {
                j += 1;
            }
            if j > k && b[j..].starts_with(b"of") {
                let mut m = j + 2;
                let k2 = m;
                while m < b.len() && ws(b[m]) {
                    m += 1;
                }
                if m > k2 && b[m..].starts_with(b"adobe") {
                    return true;
                }
            }
        }
        false
    }

    pub fn get_type1_default_permission(notice: Option<&str>, name: Option<&str>) -> Permission {
        if let Some(s) = notice {
            if Self::find_ci(s, true) {
                return Permission::Editable;
            }
            if let Some(n) = name {
                let n = Self::get_comparison_font_name(n);
                if Self::find_ci(s, false) && PUBLIC_DOMAIN_FONT_NAMES.binary_search_by(|p| p.encode_utf16().cmp(n.encode_utf16())).is_ok() {
                    return Permission::Editable;
                }
            }
        }
        Permission::PreviewAndPrint
    }

    pub fn get_cid_default_permission(a: Option<&[i32]>) -> Permission {
        match a {
            Some(a) if a.len() >= 3 => {
                if XUID_EMBED_OK_TABLE_ALL.binary_search(&(a[0], a[1], a[2])).is_ok() {
                    return Permission::PreviewAndPrint;
                }
                Permission::Restricted
            }
            _ => Permission::Restricted,
        }
    }

    pub fn get_true_type_default_permission() -> Permission {
        Permission::PreviewAndPrint
    }

    pub fn get_default_was_embedded_permission() -> Permission {
        Permission::PreviewAndPrint
    }

    pub fn get_ocf_default_permission() -> Permission {
        Permission::Restricted
    }
}
