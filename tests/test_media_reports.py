"""Evidence integrity and complete report output, without a live database."""
import base64
from io import BytesIO
from pathlib import Path
import sys
import tempfile
import unittest

from PIL import Image
from pypdf import PdfReader

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web"))
from backend.media_store import MediaStore
from backend.pdf_report import build_report
from backend.scoring import summarize


def photo_data_url():
    output = BytesIO()
    Image.new("RGB", (40, 30), "green").save(output, "PNG")
    return "data:image/png;base64," + base64.b64encode(output.getvalue()).decode()


class EvidenceReportTests(unittest.TestCase):
    def test_storage_validates_and_preserves_original_and_marked_images(self):
        with tempfile.TemporaryDirectory() as directory:
            media = MediaStore(Path(directory) / "audit.db")
            raw = photo_data_url()
            result = media.normalize({"dataUrl": raw, "markedDataUrl": raw, "name": "photo.png", "marks": [1]})
            self.assertNotIn("dataUrl", result)
            self.assertNotIn("markedDataUrl", result)
            self.assertEqual(result["marks"], [1])
            self.assertEqual(media.image_bytes(result), media.image_bytes(result, True))
            with media.connect() as db:
                self.assertEqual(db.execute("SELECT COUNT(*) FROM media_images").fetchone()[0], 1)
                self.assertEqual(db.execute("SELECT typeof(content) FROM media_images").fetchone()[0], "blob")
            self.assertEqual(media.normalize(result), result)
            for value in ("data:image/png;base64,AAAA", "https://example.com/photo.png"):
                with self.assertRaises(ValueError):
                    media.store(value)
            with self.assertRaises(ValueError):
                media.read("../../secret")

    def test_score_excludes_na_and_applies_category_weights(self):
        items = [{"category": "Safety", "passed": True}, {"category": "Other", "passed": False},
                 {"category": "Safety", "notApplicable": True}]
        settings = {"scoring.weighting": "Weighted", "scoring.weights": {"Safety": 3, "Other": 1}}
        result = summarize(items, settings)
        self.assertEqual((result["score"], result["passed"], result["failed"], result["notApplicable"]), (75, 1, 1, 1))
        self.assertTrue(result["meetsPassMark"])
        self.assertFalse(summarize([{"notApplicable": True}], {})["meetsPassMark"])

    def test_pdf_contains_final_checklist_row_images_and_signatures(self):
        with tempfile.TemporaryDirectory() as directory:
            media = MediaStore(Path(directory) / "audit.db")
            image = media.normalize({"dataUrl": photo_data_url(), "name": "Auditor"})
            items = [{"section": "Safety", "item": f"Checklist row {i}", "passed": True} for i in range(100)]
            items[0]["images"] = [image]
            items[-1]["item"] = "FINAL-CHECKLIST-ENTRY"
            report = build_report({"id": 1, "audit_ref": "AUD-TEST", "items": items,
                                   "signatures": {"auditedBy": image}}, {"companyName": "Test"}, summarize(items, {}), media)
            reader = PdfReader(BytesIO(report))
            self.assertGreater(len(reader.pages), 1)
            text = "\n".join(page.extract_text() for page in reader.pages)
            self.assertIn("FINAL-CHECKLIST-ENTRY", text)
            self.assertIn("Audited by: Auditor", text)
            self.assertGreaterEqual(sum(len(page.images) for page in reader.pages), 2)


if __name__ == "__main__":
    unittest.main()
