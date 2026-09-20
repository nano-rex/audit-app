"""One scoring calculation shared by saved audits and their reports."""
def rating_for_score(score, settings):
    excellent = float(settings.get("scoring.excellentBand", 90))
    good = float(settings.get("scoring.goodBand", 70))
    below = float(settings.get("scoring.belowBand", 60))
    return "Excellent" if score >= excellent else "Good" if score >= good else "Below Expectation" if score >= below else "Critical"


def summarize(items, settings):
    applicable = [item for item in items if not item.get("notApplicable")]
    weights = settings.get("scoring.weights", {}) if settings.get("scoring.weighting") == "Weighted" else {}
    weighted = [(item, float(weights.get(item.get("category"), 1))) for item in applicable]
    if any(weight <= 0 or weight > 100 for _, weight in weighted):
        raise ValueError("Category weights must be greater than zero and at most 100")
    total_weight = sum(weight for _, weight in weighted)
    passed_weight = sum(weight for item, weight in weighted if item.get("passed"))
    score = round(100 * passed_weight / total_weight) if total_weight else 0
    rating = rating_for_score(score, settings)
    passed = sum(bool(item.get("passed")) for item in applicable)
    return {"score": score, "rating": rating, "total": len(items), "passed": passed,
            "failed": len(applicable) - passed, "notApplicable": len(items) - len(applicable),
            "passMark": float(settings.get("scoring.passMark", 70)),
            "meetsPassMark": bool(applicable) and score >= float(settings.get("scoring.passMark", 70)),
            "weighting": settings.get("scoring.weighting", "Equal"), "weights": weights}


def validate_settings(settings):
    import math
    for key in ("passMark", "excellentBand", "goodBand", "belowBand"):
        try:
            value = float(settings[f"scoring.{key}"])
        except (KeyError, TypeError, ValueError):
            raise ValueError("Scoring thresholds must be numbers between 0 and 100") from None
        if not math.isfinite(value) or not 0 <= value <= 100:
            raise ValueError("Scoring thresholds must be numbers between 0 and 100")
    if not float(settings["scoring.excellentBand"]) > float(settings["scoring.goodBand"]) > float(settings["scoring.belowBand"]):
        raise ValueError("Excellent, Good, and Below Expectation thresholds must be in descending order")
    if settings.get("scoring.weighting") not in {"Equal", "Weighted"}:
        raise ValueError("Select Equal or Weighted scoring")
    weights = settings.get("scoring.weights", {})
    if not isinstance(weights, dict):
        raise ValueError("Category weights must map categories to numbers")
    for name, weight in weights.items():
        if not isinstance(name, str) or not isinstance(weight, (int, float)) or not math.isfinite(weight) or not 0 < weight <= 100:
            raise ValueError("Category weights must be greater than zero and at most 100")
