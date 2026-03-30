// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  devtools: { enabled: true },
  modules: ['@nuxt/eslint'],
  runtimeConfig: {
    public: {
      apiRootUrl: 'http://127.0.0.1:8080',
    },
  },
  vite: {
    optimizeDeps: {
      include: ['array-equal'],
    },
  },
})
