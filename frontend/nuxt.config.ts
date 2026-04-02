// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  devtools: { enabled: true },
  modules: ['@nuxt/eslint', '@nuxtjs/i18n'],
  runtimeConfig: {
    public: {
      apiRootUrl: 'http://127.0.0.1:8080',
    },
  },
  i18n: {
    strategy: 'no_prefix',
    experimental: {
      typedOptionsAndMessages: 'default',
    },
    baseUrl: process.env.NUXT_BASE_URL,
    detectBrowserLanguage: {
      useCookie: true,
      cookieKey: 'lang',
      fallbackLocale: 'en-US',
    },
    // Language codes: https://en.wikipedia.org/wiki/List_of_ISO_639_language_codes
    // Script codes: https://en.wikipedia.org/wiki/ISO_15924
    locales: [
      { code: 'en-US', language: 'en-US', name: 'English', dir: 'ltr', file: 'en-US.json' },
      { code: 'ja', language: 'ja', name: '日本語', dir: 'ltr', file: 'ja.json' },
      { code: 'ur-arab', language: 'ur-Arab', name: 'اُردوٗ', dir: 'rtl', file: 'ur-Arab.json' },
    ],
  },
  vite: {
    optimizeDeps: {
      include: ['array-equal', 'ts-pattern'],
    },
  },
})
