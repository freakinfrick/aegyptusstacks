"""z-band computation: CHRONOLOGICAL era assignment.

z-band meaning (chronological — same topic at same q,r across eras):
  z=+3: Observatory (entry point, single room)
  z=+2: Gallery (district overviews)
  z=+1: Atrium (cluster summaries)
  z= 0: Undated / mixed period (surface)
  z=-1: Old Kingdom (c. 2686-2181 BCE)
  z=-2: First Intermediate (c. 2181-2055 BCE)
  z=-3: Middle Kingdom (c. 2055-1650 BCE)
  z=-4: Second Intermediate (c. 1650-1550 BCE)
  z=-5: New Kingdom (c. 1550-1069 BCE)
  z=-6: Late Period (c. 1069-332 BCE)
  z=-7: Greco-Roman (c. 332 BCE-395 CE)
"""

import re

# Period label regex rules — ordered most specific first.
# Each rule: (compiled_regex, z_band)
_ERA_RULES: list[tuple[re.Pattern, int]] = [
    # L7 Greco-Roman (z=-7)
    (re.compile(r'ptolem|makedon|ptolemä|greek|hellenist', re.I), -7),
    (re.compile(r'\brome?\b|roman|römisch', re.I), -7),
    (re.compile(r'coptic|koptisch', re.I), -7),
    (re.compile(r'\b(3|4)\.?\s*jhdt\.?\s*v\.?\s*chr', re.I), -7),
    (re.compile(r'\b(3rd|4th)\s+cent', re.I), -7),

    # L6 Late Period / Third Intermediate (z=-6)
    (re.compile(r'saite|spätzeit|late\s+period', re.I), -6),
    (re.compile(r'dritte\s+zwischenzeit|third\s+intermediate', re.I), -6),
    (re.compile(r'kushite|nubian|nubisch', re.I), -6),
    (re.compile(r'^(2[1-9]|30)\.?\s*dyn', re.I), -6),
    (re.compile(r'\b(21st|22nd|23rd|24th|25th|26th|27th|28th|29th|30th)\s+dynasty', re.I), -6),
    (re.compile(r'taharqa|pije|piye|scheschonq|shoshenq|psammetich|apries|necho|amasis', re.I), -6),

    # L5 New Kingdom (z=-5)
    (re.compile(r'new\s+kingdom|neues\s+reich|amarna|ramesside|ramessid', re.I), -5),
    (re.compile(r'^(18|19|20)\.?\s*dyn', re.I), -5),
    (re.compile(r'\b(18th|19th|20th)\s+dynasty', re.I), -5),
    (re.compile(r'ahmose|amenophis|amenhotep|hatschepsut|thutmosis|tutanchamun', re.I), -5),
    (re.compile(r'akhenaten|echnaton|ramses|merenptah|sethos|seti', re.I), -5),

    # L4 Second Intermediate (z=-4)
    (re.compile(r'second\s+intermediate|zweite\s+zwischenzeit|hyksos', re.I), -4),
    (re.compile(r'^(1[3-7])[\s\./]', re.I), -4),

    # L3 Middle Kingdom (z=-3)
    (re.compile(r'middle\s+kingdom|mittleres\s+reich', re.I), -3),
    (re.compile(r'^(11|12)\.?\s*dyn', re.I), -3),
    (re.compile(r'\b(11th|12th)\s+dynasty', re.I), -3),
    (re.compile(r'amenemhet|sesostris|senwosret|sebekhotep|mentuhotep', re.I), -3),

    # L2 First Intermediate (z=-2)
    (re.compile(r'first\s+intermediate|erste\s+zwischenzeit|herakleopol', re.I), -2),
    (re.compile(r'^([7-9]|10)\.?\s*dyn', re.I), -2),
    (re.compile(r'9\./10\.\s*dyn', re.I), -2),

    # L1 Old Kingdom (z=-1)
    (re.compile(r'old\s+kingdom|altes\s+reich', re.I), -1),
    (re.compile(r'early\s+dynastic|frühdynastisch|archaic', re.I), -1),
    (re.compile(r'^[1-6]\.?\s*dyn', re.I), -1),
    (re.compile(r'\b(1st|2nd|3rd|4th|5th|6th)\s+dynasty', re.I), -1),
    (re.compile(r'djoser|snofru|cheops|khufu|chefren|khafre|mykerinos|pepi|teti|unas', re.I), -1),
]


def period_to_z(period_label: str | None) -> int:
    """Map a period_label string to chronological z-band.

    Returns z in [-7, 0]. 0 = undated/unknown.
    """
    if not period_label:
        return 0
    for pattern, z in _ERA_RULES:
        if pattern.search(period_label):
            return z
    return 0  # unrecognized period → surface


def compute_z_band(
    canonical_id: str,
    period_label: str | None,
    landmarks: dict[str, int],
) -> int:
    """Compute chronological z-band.

    Priority: landmark override > period_label regex matching.

    Args:
        canonical_id: Text identifier.
        period_label: Free-form period string from corpus metadata.
        landmarks: Mapping of canonical_id -> fixed z_band.

    Returns:
        Integer z-band in [-7, 0].
    """
    # Priority 1: Landmark override
    if canonical_id in landmarks:
        return landmarks[canonical_id]

    # Priority 2: Chronological era from period_label
    return period_to_z(period_label)
