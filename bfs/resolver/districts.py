"""District layout definitions for the BFS hex-grid q/r plane.

Each district has a center point (q, r) on the hex grid and a radius
proportional to its text count. The arrangement mirrors the Egyptian
institutional landscape: temple central, necropolis distant, school
near admin, quarry peripheral.
"""

from dataclasses import dataclass


@dataclass(frozen=True)
class DistrictRegion:
    """A district's position and extent on the hex grid."""
    name: str
    center_q: int
    center_r: int
    radius: int
    description: str


# Centers and radii calibrated from Phase 1 census + Phase 2 spec.
# Radii derived from: ceil(sqrt(text_count / MAX_VOLUMES_PER_ROOM / pi)) + margin
DISTRICT_LAYOUT: dict[str, DistrictRegion] = {
    "temple": DistrictRegion(
        name="temple",
        center_q=0,
        center_r=0,
        radius=30,
        description="Temple inscriptions, liturgy, hymns, architectural elements",
    ),
    "house_of_life": DistrictRegion(
        name="house_of_life",
        center_q=18,
        center_r=-5,
        radius=20,
        description="Medical, magical, ritual scrolls — the pr-ankh",
    ),
    "admin": DistrictRegion(
        name="admin",
        center_q=-15,
        center_r=12,
        radius=18,
        description="Letters, legal, economic texts — administrative quarter",
    ),
    "necropolis": DistrictRegion(
        name="necropolis",
        center_q=25,
        center_r=20,
        radius=25,
        description="Tombs, coffins, false doors, funerary texts",
    ),
    "school": DistrictRegion(
        name="school",
        center_q=-10,
        center_r=18,
        radius=6,
        description="Student exercises, model texts, ostraca, writing boards",
    ),
    "quarry": DistrictRegion(
        name="quarry",
        center_q=-30,
        center_r=-15,
        radius=8,
        description="Rock inscriptions, quarry marks, expedition graffiti",
    ),
    "royal": DistrictRegion(
        name="royal",
        center_q=5,
        center_r=-18,
        radius=14,
        description="Royal sculpture, monumental stele, decrees",
    ),
    "domestic": DistrictRegion(
        name="domestic",
        center_q=-20,
        center_r=-5,
        radius=10,
        description="Vessels, artifacts, household objects, personal items",
    ),
    "unplaced": DistrictRegion(
        name="unplaced",
        center_q=-40,
        center_r=0,
        radius=30,
        description="Texts without sufficient metadata for district assignment",
    ),
}
