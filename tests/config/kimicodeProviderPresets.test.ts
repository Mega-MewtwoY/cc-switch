import { describe, expect, it } from "vitest";
import {
  kimicodeProviderPresets,
  KIMICODE_DEFAULT_CONFIG,
} from "@/config/kimicodeProviderPresets";

describe("Kimi Code Provider Presets", () => {
  it("should export a default config string", () => {
    const parsed = JSON.parse(KIMICODE_DEFAULT_CONFIG);
    expect(parsed.provider).toMatchObject({
      type: "openai",
      api_key: "",
      base_url: "",
    });
    expect(parsed.models).toEqual({});
    expect(parsed.default_model).toBe("");
  });

  it("should include Moonshot official preset", () => {
    const preset = kimicodeProviderPresets.find(
      (p) => p.nameKey === "providerForm.presets.moonshotOfficial",
    );
    expect(preset).toBeDefined();
    expect(preset!.category).toBe("cn_official");
    expect(preset!.settingsConfig.provider.type).toBe("openai");
    expect(preset!.settingsConfig.provider.base_url).toBe(
      "https://api.moonshot.cn/v1",
    );
    expect(preset!.settingsConfig.models).toHaveProperty("kimi-k2.6");
    expect(preset!.settingsConfig.default_model).toBe("kimi-k2.6");
  });

  it("should include Moonshot global preset", () => {
    const preset = kimicodeProviderPresets.find(
      (p) => p.nameKey === "providerForm.presets.moonshotGlobal",
    );
    expect(preset).toBeDefined();
    expect(preset!.settingsConfig.provider.base_url).toBe(
      "https://api.moonshot.ai/v1",
    );
  });

  it("should include custom preset", () => {
    const preset = kimicodeProviderPresets.find(
      (p) => p.nameKey === "providerForm.presets.custom",
    );
    expect(preset).toBeDefined();
    expect(preset!.category).toBe("custom");
  });
});
