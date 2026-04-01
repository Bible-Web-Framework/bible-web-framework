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
      fallbackLocale: 'en',
    },
    locales: [
      { code: 'en', language: 'en', name: 'English', dir: 'ltr', file: 'en.json' },
      { code: 'ur-arab', language: 'ur-Arab', name: 'اُردوٗ', dir: 'rtl', file: 'ur-arab.json' },
    ],
  },
  vite: {
    optimizeDeps: {
      include: ['array-equal'],
    },
  },
})
