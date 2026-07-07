import { describe, expect, it } from "vitest";
import { APPEARANCE_SETTINGS_SECTION } from "./settingsSections";

describe("APPEARANCE_SETTINGS_SECTION", () => {
  it("exposes Brio and Catppuccin as selectable Appearance themes", () => {
    const themeField = APPEARANCE_SETTINGS_SECTION.fields.find(
      (field) => field.key === "ui.theme",
    );

    expect(themeField).toMatchObject({
      field_type: "select",
      label: "Color theme",
    });
    expect(themeField?.options).toEqual([
      { value: "brio", label: "Brio" },
      { value: "catppuccin-mocha", label: "Catppuccin Mocha" },
    ]);
  });
});
