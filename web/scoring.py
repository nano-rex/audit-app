"""One scoring calculation shared by saved audits and their reports."""
def summarize(items, settings):
    applicable = [item for item in items if not item.get("notApplicable")]
    weights = settings.get("scoring.weights", {}) if settings.get("scoring.weighting") == "Weighted" else {}
    weighted = [(item, float(weights.get(item.get("category"), 1))) for item in applicable]
    if any(weight <= 0 or weight > 100 for _, weight in weighted):
        raise ValueError("Category weights must be greater than zero and at most 100")
    total_weight = sum(weight for _, weight in weighted)
    passed_weight = sum(weight for item, weight in weighted if item.get("passed"))
    score = round(100 * passed_weight / total_weight) if total_weight else 0
    excellent = float(settings.get("scoring.excellentBand", 90))
    good = float(settings.get("scoring.goodBand", 70))
    below = float(settings.get("scoring.belowBand", 60))
    rating = "Excellent" if score >= excellent else "Good" if score >= good else "Below Expectation" if score >= below else "Critical"
    passed = sum(bool(item.get("passed")) for item in applicable)
    return {"score": score, "rating": rating, "total": len(items), "passed": passed,
            "failed": len(applicable) - passed, "notApplicable": len(items) - len(applicable),
            "passMark": float(settings.get("scoring.passMark", 70)),
            "meetsPassMark": bool(applicable) and score >= float(settings.get("scoring.passMark", 70)),
            "weighting": settings.get("scoring.weighting", "Equal"), "weights": weights}
