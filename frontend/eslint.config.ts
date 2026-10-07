import pluginVue from 'eslint-plugin-vue'
import { defineConfigWithVueTs, vueTsConfigs } from '@vue/eslint-config-typescript'
import oxlint from 'eslint-plugin-oxlint'
import skipFormatting from '@vue/eslint-config-prettier/skip-formatting'

export default defineConfigWithVueTs(
  {
    name: 'app/files-to-lint',
    files: ['**/*.{ts,mts,tsx,vue}'],
  },

  {
    name: 'app/files-to-ignore',
    ignores: ['**/dist/**', '**/dist-ssr/**', '**/coverage/**', 'src/services/api-schema/**'],
  },

  pluginVue.configs['flat/essential'],
  vueTsConfigs.recommended,

  ...oxlint.configs['flat/recommended'],
  skipFormatting,

  {
    rules: {
      'vue/no-reserved-component-names': 'off',
    },
  },
)
