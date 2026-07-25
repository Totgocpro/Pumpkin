use std::{
    borrow::Cow,
    collections::HashMap,
    path::PathBuf,
    str::FromStr,
    sync::{LazyLock, Mutex},
};

use crate::text::{TextComponentBase, TextContent, style::Style};
use crate::asset_path;

/// Load translation JSON from a file path, returning an empty map on failure.
fn load_lang_json(path: &PathBuf) -> HashMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

/// Load vanilla translations from the Mojang asset cache and merge them
/// into the global translation table. Must be called AFTER `asset_path::set_cache_root()`.
pub fn init_vanilla_translations() {
    let mut translations = TRANSLATIONS.lock().unwrap();

    // en_us from Mojang cache
    if let Some(lang_dir) = asset_path::lang_dir() {
        let path = lang_dir.join("en_us.json");
        let vanilla = load_lang_json(&path);
        for (key, value) in vanilla {
            let prefixed = if key.contains(':') {
                key.to_lowercase()
            } else {
                format!("minecraft:{}", key).to_lowercase()
            };
            translations[Locale::EnUs as usize].insert(prefixed, value);
        }
    }
}

pub static TRANSLATIONS: LazyLock<Mutex<[HashMap<String, String>; Locale::COUNT]>> =
    LazyLock::new(|| {
        let mut array: [HashMap<String, String>; Locale::COUNT] =
            std::array::from_fn(|_| HashMap::new());

        // Inline pumpkin-specific translations only
        let inline_pumpkin: &[(&str, &str, &str)] = &[
            ("en_us", "pumpkin", include_str!("../../assets/translations/en_us.json")),
            ("brb", "pumpkin", include_str!("../../assets/translations/brb.json")),
            ("de_de", "pumpkin", include_str!("../../assets/translations/de_de.json")),
            ("es_es", "pumpkin", include_str!("../../assets/translations/es_es.json")),
            ("fr_fr", "pumpkin", include_str!("../../assets/translations/fr_fr.json")),
            ("it_it", "pumpkin", include_str!("../../assets/translations/it_it.json")),
            ("ja_jp", "pumpkin", include_str!("../../assets/translations/ja_jp.json")),
            ("ka_ge", "pumpkin", include_str!("../../assets/translations/ka_ge.json")),
            ("ko_kr", "pumpkin", include_str!("../../assets/translations/ko_kr.json")),
            ("nds_de", "pumpkin", include_str!("../../assets/translations/nds_de.json")),
            ("nl_be", "pumpkin", include_str!("../../assets/translations/nl_be.json")),
            ("nl_nl", "pumpkin", include_str!("../../assets/translations/nl_nl.json")),
            ("ro_ro", "pumpkin", include_str!("../../assets/translations/ro_ro.json")),
            ("ru_ru", "pumpkin", include_str!("../../assets/translations/ru_ru.json")),
            ("sq_al", "pumpkin", include_str!("../../assets/translations/sq_al.json")),
            ("zh_cn", "pumpkin", include_str!("../../assets/translations/zh_cn.json")),
            ("zh_hk", "pumpkin", include_str!("../../assets/translations/zh_hk.json")),
            ("zh_tw", "pumpkin", include_str!("../../assets/translations/zh_tw.json")),
            ("lzh", "pumpkin", include_str!("../../assets/translations/lzh.json")),
            ("tr_tr", "pumpkin", include_str!("../../assets/translations/tr_tr.json")),
            ("uk_ua", "pumpkin", include_str!("../../assets/translations/uk_ua.json")),
            ("vi_vn", "pumpkin", include_str!("../../assets/translations/vi_vn.json")),
            ("pt_br", "pumpkin", include_str!("../../assets/translations/pt_br.json")),
            ("pl_pl", "pumpkin", include_str!("../../assets/translations/pl_pl.json")),
        ];

        let locale_map: HashMap<&str, Locale> = [
            ("en_us", Locale::EnUs),
            ("brb", Locale::Brb),
            ("de_de", Locale::DeDe),
            ("es_es", Locale::EsEs),
            ("fr_fr", Locale::FrFr),
            ("it_it", Locale::ItIt),
            ("ja_jp", Locale::JaJp),
            ("ka_ge", Locale::KaGe),
            ("ko_kr", Locale::KoKr),
            ("nds_de", Locale::NdsDe),
            ("nl_be", Locale::NlBe),
            ("nl_nl", Locale::NlNl),
            ("ro_ro", Locale::RoRo),
            ("ru_ru", Locale::RuRu),
            ("sq_al", Locale::SqAl),
            ("zh_cn", Locale::ZhCn),
            ("zh_hk", Locale::ZhHk),
            ("zh_tw", Locale::ZhTw),
            ("lzh", Locale::Lzh),
            ("tr_tr", Locale::TrTr),
            ("uk_ua", Locale::UkUa),
            ("vi_vn", Locale::ViVn),
            ("pt_br", Locale::PtBr),
            ("pl_pl", Locale::PlPl),
        ]
        .into_iter()
        .collect();

        for (locale_str, namespace, json_str) in inline_pumpkin {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(json_str) {
                let locale_idx = locale_map.get(locale_str).copied().unwrap_or(Locale::EnUs) as usize;
                for (key, value) in map {
                    array[locale_idx].insert(format!("{namespace}:{key}"), value);
                }
            }
        }

        Mutex::new(array)
    });

/// Adds or overrides a single translation entry.
pub fn add_translation<P: Into<String>>(namespace: P, key: P, translation: P, locale: Locale) {
    let mut translations = TRANSLATIONS.lock().unwrap();
    let namespaced_key = format!("{}:{}", namespace.into(), key.into()).to_lowercase();
    translations[locale as usize].insert(namespaced_key, translation.into());
}

/// Loads translations from a JSON string and registers them under a namespace.
pub fn add_translation_file<P: Into<String>>(namespace: P, file_path: P, locale: Locale) {
    let translations_map: HashMap<String, String> =
        serde_json::from_str(&file_path.into()).unwrap_or(HashMap::new());
    if translations_map.is_empty() {
        return;
    }
    let mut translations = TRANSLATIONS.lock().unwrap();
    let namespace = namespace.into();
    for (key, translation) in translations_map {
        let namespaced_key = format!("{namespace}:{key}").to_lowercase();
        translations[locale as usize].insert(namespaced_key, translation);
    }
}

/// Retrieves a translation for the given key and locale.
pub fn get_translation(key: &str, locale: Locale) -> String {
    let translations = TRANSLATIONS.lock().unwrap();
    let key = key.to_lowercase();
    translations[locale as usize].get(&key).map_or_else(
        || {
            translations[Locale::EnUs as usize]
                .get(&key)
                .map_or(key, Clone::clone)
        },
        Clone::clone,
    )
}

/// Reorders substitution placeholders within a translation string.
#[must_use]
pub fn reorder_substitutions(
    translation: &str,
    with: Vec<TextComponentBase>,
) -> (Vec<TextComponentBase>, Vec<SubstitutionRange>) {
    let indices: Vec<usize> = translation
        .match_indices('%')
        .filter(|(i, _)| *i == 0 || translation.as_bytes()[i - 1] != b'\\')
        .map(|(i, _)| i)
        .collect();

    if translation.matches("%s").count() == indices.len() {
        return (
            with,
            indices
                .iter()
                .map(|&i| SubstitutionRange {
                    start: i,
                    end: i + 1,
                })
                .collect(),
        );
    }

    let mut substitutions: Vec<TextComponentBase> = indices
        .iter()
        .map(|_| TextComponentBase {
            content: Box::new(TextContent::Text { text: "".into() }),
            style: Box::new(Style::default()),
            extra: vec![],
        })
        .collect();
    let mut ranges: Vec<SubstitutionRange> = vec![];

    let bytes = translation.as_bytes();
    let mut next_idx = 0usize;
    for (idx, &i) in indices.iter().enumerate() {
        let mut num_chars = String::new();
        let mut pos = 1;
        while i + pos < bytes.len() && bytes[i + pos].is_ascii_digit() {
            num_chars.push(bytes[i + pos] as char);
            pos += 1;
        }

        if num_chars.is_empty() {
            ranges.push(SubstitutionRange {
                start: i,
                end: i + 1,
            });
            substitutions[idx] = with[next_idx].clone();
            next_idx = (next_idx + 1).clamp(0, with.len() - 1);
            continue;
        }

        ranges.push(SubstitutionRange {
            start: i,
            end: i + pos + 1,
        });
        if let Ok(digit) = num_chars.parse::<usize>() {
            substitutions[idx] = with[digit.clamp(1, with.len()) - 1].clone();
        }
    }
    (substitutions, ranges)
}

/// Resolves a translation into formatted console output.
pub fn translation_to_pretty<P: Into<Cow<'static, str>>>(
    namespaced_key: P,
    locale: Locale,
    with: Vec<TextComponentBase>,
) -> String {
    let translation = get_translation(&namespaced_key.into(), locale);
    if with.is_empty() || !translation.contains('%') {
        return translation;
    }

    let (substitutions, indices) = reorder_substitutions(&translation, with);
    let mut result = String::new();
    let mut pos = 0;

    for (idx, &range) in indices.iter().enumerate() {
        let sub_idx = idx.clamp(0, substitutions.len() - 1);
        let substitution = substitutions[sub_idx].clone().to_pretty_console();

        result.push_str(&translation[pos..range.start]);
        result.push_str(&substitution);
        pos = range.end + 1;
    }

    result.push_str(&translation[pos..]);
    result
}

/// Resolves a translation into plain text.
pub fn get_translation_text<P: Into<Cow<'static, str>>>(
    namespaced_key: P,
    locale: Locale,
    with: Vec<TextComponentBase>,
) -> String {
    let translation = get_translation(&namespaced_key.into(), locale);
    if with.is_empty() || !translation.contains('%') {
        return translation;
    }

    let (substitutions, indices) = reorder_substitutions(&translation, with);
    let mut result = String::new();
    let mut pos = 0;

    for (idx, &range) in indices.iter().enumerate() {
        let sub_idx = idx.clamp(0, substitutions.len() - 1);
        let substitution = substitutions[sub_idx].clone().get_text(locale);

        result.push_str(&translation[pos..range.start]);
        result.push_str(&substitution);
        pos = range.end + 1;
    }

    result.push_str(&translation[pos..]);
    result
}

/// A character range representing a substitution placeholder within a translation string.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SubstitutionRange {
    pub start: usize,
    pub end: usize,
}
impl SubstitutionRange {
    #[must_use]
    pub const fn len(&self) -> usize {
        (self.end - self.start) + 1
    }
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Supported locales for translations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Locale {
    AfZa, ArSa, AstEs, AzAz, BaRu, Bar, BeBy, BgBg, BrFr, Brb, BsBa, CaEs, CsCz, CyGb,
    DaDk, DeAt, DeCh, DeDe, ElGr, EnAu, EnCa, EnGb, EnNz, EnPt, EnUd, EnUs, Enp, Enws,
    EoUy, EsAr, EsCl, EsEc, EsEs, EsMx, EsUy, EsVe, Esan, EtEe, EuEs, FaIr, FiFi, FilPh,
    FoFo, FrCa, FrFr, FraDe, FurIt, FyNl, GaIe, GdGb, GlEs, HawUs, HeIl, HiIn, HrHr, HuHu,
    HyAm, IdId, IgNg, IoEn, IsIs, Isv, ItIt, JaJp, JboEn, KaGe, KkKz, KnIn, KoKr, Ksh,
    KwGb, LaLa, LbLu, LiLi, Lmo, LoLa, LolUs, LtLt, LvLv, Lzh, MkMk, MnMn, MsMy, MtMt,
    Nah, NdsDe, NlBe, NlNl, NnNo, NoNo, OcFr, Ovd, PlPl, PtBr, PtPt, QyaAa, RoRo, Rpr,
    RuRu, RyUa, SahSah, SeNo, SkSk, SlSi, SoSo, SqAl, SrCs, SrSp, SvSe, Sxu, Szl, TaIn,
    ThTh, TlPh, TlhAa, Tok, TrTr, TtRu, UkUa, ValEs, VecIt, ViVn, YiDe, YoNg, ZhCn, ZhHk,
    ZhTw, ZlmArab,
}

impl Locale {
    pub const COUNT: usize = Self::ZlmArab as usize + 1;
}

impl FromStr for Locale {
    type Err = ();

    #[expect(clippy::too_many_lines)]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "af_za" => Ok(Self::AfZa),
            "ar_sa" => Ok(Self::ArSa),
            "ast_es" => Ok(Self::AstEs),
            "az_az" => Ok(Self::AzAz),
            "ba_ru" => Ok(Self::BaRu),
            "bar" => Ok(Self::Bar),
            "be_by" => Ok(Self::BeBy),
            "bg_bg" => Ok(Self::BgBg),
            "br_fr" => Ok(Self::BrFr),
            "brb" => Ok(Self::Brb),
            "bs_ba" => Ok(Self::BsBa),
            "ca_es" => Ok(Self::CaEs),
            "cs_cz" => Ok(Self::CsCz),
            "cy_gb" => Ok(Self::CyGb),
            "da_dk" => Ok(Self::DaDk),
            "de_at" => Ok(Self::DeAt),
            "de_ch" => Ok(Self::DeCh),
            "de_de" => Ok(Self::DeDe),
            "el_gr" => Ok(Self::ElGr),
            "en_au" => Ok(Self::EnAu),
            "en_ca" => Ok(Self::EnCa),
            "en_gb" => Ok(Self::EnGb),
            "en_nz" => Ok(Self::EnNz),
            "en_pt" => Ok(Self::EnPt),
            "en_ud" => Ok(Self::EnUd),
            "enp" => Ok(Self::Enp),
            "enws" => Ok(Self::Enws),
            "eo_uy" => Ok(Self::EoUy),
            "es_ar" => Ok(Self::EsAr),
            "es_cl" => Ok(Self::EsCl),
            "es_ec" => Ok(Self::EsEc),
            "es_es" => Ok(Self::EsEs),
            "es_mx" => Ok(Self::EsMx),
            "es_uy" => Ok(Self::EsUy),
            "es_ve" => Ok(Self::EsVe),
            "esan" => Ok(Self::Esan),
            "et_ee" => Ok(Self::EtEe),
            "eu_es" => Ok(Self::EuEs),
            "fa_ir" => Ok(Self::FaIr),
            "fi_fi" => Ok(Self::FiFi),
            "fil_ph" => Ok(Self::FilPh),
            "fo_fo" => Ok(Self::FoFo),
            "fr_ca" => Ok(Self::FrCa),
            "fr_fr" => Ok(Self::FrFr),
            "fra_de" => Ok(Self::FraDe),
            "fur_it" => Ok(Self::FurIt),
            "fy_nl" => Ok(Self::FyNl),
            "ga_ie" => Ok(Self::GaIe),
            "gd_gb" => Ok(Self::GdGb),
            "gl_es" => Ok(Self::GlEs),
            "haw_us" => Ok(Self::HawUs),
            "he_il" => Ok(Self::HeIl),
            "hi_in" => Ok(Self::HiIn),
            "hr_hr" => Ok(Self::HrHr),
            "hu_hu" => Ok(Self::HuHu),
            "hy_am" => Ok(Self::HyAm),
            "id_id" => Ok(Self::IdId),
            "ig_ng" => Ok(Self::IgNg),
            "io_en" => Ok(Self::IoEn),
            "is_is" => Ok(Self::IsIs),
            "isv" => Ok(Self::Isv),
            "it_it" => Ok(Self::ItIt),
            "ja_jp" => Ok(Self::JaJp),
            "jbo_en" => Ok(Self::JboEn),
            "ka_ge" => Ok(Self::KaGe),
            "kk_kz" => Ok(Self::KkKz),
            "kn_in" => Ok(Self::KnIn),
            "ko_kr" => Ok(Self::KoKr),
            "ksh" => Ok(Self::Ksh),
            "kw_gb" => Ok(Self::KwGb),
            "la_la" => Ok(Self::LaLa),
            "lb_lu" => Ok(Self::LbLu),
            "li_li" => Ok(Self::LiLi),
            "lmo" => Ok(Self::Lmo),
            "lo_la" => Ok(Self::LoLa),
            "lol_us" => Ok(Self::LolUs),
            "lt_lt" => Ok(Self::LtLt),
            "lv_lv" => Ok(Self::LvLv),
            "lzh" => Ok(Self::Lzh),
            "mk_mk" => Ok(Self::MkMk),
            "mn_mn" => Ok(Self::MnMn),
            "ms_my" => Ok(Self::MsMy),
            "mt_mt" => Ok(Self::MtMt),
            "nah" => Ok(Self::Nah),
            "nds_de" => Ok(Self::NdsDe),
            "nl_be" => Ok(Self::NlBe),
            "nl_nl" => Ok(Self::NlNl),
            "nn_no" => Ok(Self::NnNo),
            "no_no" => Ok(Self::NoNo),
            "oc_fr" => Ok(Self::OcFr),
            "ovd" => Ok(Self::Ovd),
            "pl_pl" => Ok(Self::PlPl),
            "pt_br" => Ok(Self::PtBr),
            "pt_pt" => Ok(Self::PtPt),
            "qya_aa" => Ok(Self::QyaAa),
            "ro_ro" => Ok(Self::RoRo),
            "rpr" => Ok(Self::Rpr),
            "ru_ru" => Ok(Self::RuRu),
            "ry_ua" => Ok(Self::RyUa),
            "sah_sah" => Ok(Self::SahSah),
            "se_no" => Ok(Self::SeNo),
            "sk_sk" => Ok(Self::SkSk),
            "sl_si" => Ok(Self::SlSi),
            "so_so" => Ok(Self::SoSo),
            "sq_al" => Ok(Self::SqAl),
            "sr_cs" => Ok(Self::SrCs),
            "sr_sp" => Ok(Self::SrSp),
            "sv_se" => Ok(Self::SvSe),
            "sxu" => Ok(Self::Sxu),
            "szl" => Ok(Self::Szl),
            "ta_in" => Ok(Self::TaIn),
            "th_th" => Ok(Self::ThTh),
            "tl_ph" => Ok(Self::TlPh),
            "tlh_aa" => Ok(Self::TlhAa),
            "tok" => Ok(Self::Tok),
            "tr_tr" => Ok(Self::TrTr),
            "tt_ru" => Ok(Self::TtRu),
            "uk_ua" => Ok(Self::UkUa),
            "val_es" => Ok(Self::ValEs),
            "vec_it" => Ok(Self::VecIt),
            "vi_vn" => Ok(Self::ViVn),
            "yi_de" => Ok(Self::YiDe),
            "yo_ng" => Ok(Self::YoNg),
            "zh_cn" => Ok(Self::ZhCn),
            "zh_hk" => Ok(Self::ZhHk),
            "zh_tw" => Ok(Self::ZhTw),
            "zlm_arab" => Ok(Self::ZlmArab),
            _ => Ok(Self::EnUs),
        }
    }
}
