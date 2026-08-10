import type { ProviderCategory, KimiCodeProviderConfig } from "../types";
import type { PresetTheme, TemplateValueConfig } from "./claudeProviderPresets";

export interface KimiCodeProviderPreset {
  name: string;
  nameKey?: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: KimiCodeProviderConfig;
  isOfficial?: boolean;
  isPartner?: boolean;
  primePartner?: boolean;
  partnerPromotionKey?: string;
  category?: ProviderCategory;
  templateValues?: Record<string, TemplateValueConfig>;
  theme?: PresetTheme;
  icon?: string;
  iconColor?: string;
  isCustomTemplate?: boolean;
}

export const KIMICODE_DEFAULT_CONFIG = JSON.stringify(
  {
    provider: {
      type: "openai",
      api_key: "",
      base_url: "",
    },
    models: {},
    default_model: "",
  },
  null,
  2,
);

export const kimicodeProviderPresets: KimiCodeProviderPreset[] = [
  {
    name: "Moonshot 官方（API Key）",
    nameKey: "providerForm.presets.moonshotOfficial",
    websiteUrl: "https://platform.moonshot.cn",
    apiKeyUrl: "https://platform.moonshot.cn/console/api-keys",
    settingsConfig: {
      provider: {
        type: "openai",
        api_key: "",
        base_url: "https://api.moonshot.cn/v1",
      },
      models: {
        "kimi-k2.6": {
          model: "kimi-k2.6",
          max_context_size: 262144,
          capabilities: ["thinking", "tool_use"],
          display_name: "Kimi K2.6",
        },
      },
      default_model: "kimi-k2.6",
    },
    category: "cn_official",
    isOfficial: true,
    icon: "kimi",
    iconColor: "#6366F1",
    templateValues: {
      apiKey: {
        label: "API Key",
        placeholder: "sk-...",
        editorValue: "",
      },
    },
  },
  {
    name: "Moonshot 国际站",
    nameKey: "providerForm.presets.moonshotGlobal",
    websiteUrl: "https://platform.moonshot.ai",
    apiKeyUrl: "https://platform.moonshot.ai/console/api-keys",
    settingsConfig: {
      provider: {
        type: "openai",
        api_key: "",
        base_url: "https://api.moonshot.ai/v1",
      },
      models: {
        "kimi-k2.6": {
          model: "kimi-k2.6",
          max_context_size: 262144,
          capabilities: ["thinking", "tool_use"],
          display_name: "Kimi K2.6",
        },
      },
      default_model: "kimi-k2.6",
    },
    category: "cn_official",
    isOfficial: true,
    icon: "kimi",
    iconColor: "#6366F1",
    templateValues: {
      apiKey: {
        label: "API Key",
        placeholder: "sk-...",
        editorValue: "",
      },
    },
  },
  {
    name: "Custom",
    nameKey: "providerForm.presets.custom",
    websiteUrl: "",
    settingsConfig: {
      provider: {
        type: "openai",
        api_key: "",
        base_url: "",
      },
      models: {},
      default_model: "",
    },
    category: "custom",
    isCustomTemplate: true,
    icon: "settings",
  },
];
