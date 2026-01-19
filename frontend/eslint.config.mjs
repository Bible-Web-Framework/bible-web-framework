import withNuxt from './.nuxt/eslint.config.mjs'

export default withNuxt({
  rules: {
    eqeqeq: 'error',
    'vue/eqeqeq': 'error',
    'vue/html-self-closing': 'off',
  },
})
