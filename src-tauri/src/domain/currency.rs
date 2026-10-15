// ISO 4217 current currency/funds codes (excluding XTS test and XXX no currency).
// Source: https://www.six-group.com/dam/download/financial-information/data-center/iso-currrency/lists/list-one.xml
// Source published: 2026-09-17; retrieved: 2026-10-15. Refresh from SIX when updating.
// Kept offline: account refresh never downloads a code list.
const CODES: &str = "AED AFN ALL AMD AOA ARS AUD AWG AZN BAM BBD BDT BHD BIF BMD BND BOB BOV BRL BSD BTN BWP BYN BZD CAD CDF CHE CHF CHW CLF CLP CNY COP COU CRC CUP CVE CZK DJF DKK DOP DZD EGP ERN ETB EUR FJD FKP GBP GEL GHS GIP GMD GNF GTQ GYD HKD HNL HTG HUF IDR ILS INR IQD IRR ISK JMD JOD JPY KES KGS KHR KMF KPW KRW KWD KYD KZT LAK LBP LKR LRD LSL LYD MAD MDL MGA MKD MMK MNT MOP MRU MUR MVR MWK MXN MXV MYR MZN NAD NGN NIO NOK NPR NZD OMR PAB PEN PGK PHP PKR PLN PYG QAR RON RSD RUB RWF SAR SBD SCR SDG SEK SGD SHP SLE SOS SRD SSP STN SVC SYP SZL THB TJS TMT TND TOP TRY TTD TWD TZS UAH UGX USD USN UYI UYU UYW UZS VED VES VND VUV WST XAD XAF XAG XAU XBA XBB XBC XBD XCD XCG XDR XOF XPD XPF XPT XSU XUA YER ZAR ZMW ZWG";

pub fn normalize(value: &str) -> Option<String> {
    let code = value.trim().to_ascii_uppercase();
    CODES
        .split_ascii_whitespace()
        .any(|known| known == code)
        .then_some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_supported_codes_without_guessing() {
        for code in ["CNY", "USD", "JPY", "EUR", "GBP", "HKD"] {
            assert_eq!(normalize(code).as_deref(), Some(code));
        }
        assert_eq!(normalize(" eur ").as_deref(), Some("EUR"));
        for value in [
            "",
            "ZZZ",
            "XXX",
            "XTS",
            "人民币",
            "US",
            "USDD",
            "currency_unknown",
        ] {
            assert_eq!(normalize(value), None);
        }
    }
}
