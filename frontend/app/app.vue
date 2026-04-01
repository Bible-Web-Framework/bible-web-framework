<script setup lang="ts">
const { locales, locale } = useI18n()
const head = useLocaleHead({
  dir: true,
  seo: false,
})
</script>

<template>
  <Html :dir="head.htmlAttrs.dir">
    <NuxtLoadingIndicator />
    <NuxtLayout>
      <nav style="display: flex; gap: 10px">
        <NuxtLink to="/">{{ $t('page.home') }}</NuxtLink>
        <NuxtLink to="/search">{{ $t('page.search') }}</NuxtLink>
        <select v-model="locale">
          <option
            v-for="localeOption in locales"
            :key="localeOption.code"
            :value="localeOption.code"
          >
            {{ localeOption.name }}
          </option>
        </select>
      </nav>
      <NuxtPage class="centered-body" />
    </NuxtLayout>
  </Html>
</template>

<style>
@import url('~/assets/bwf.css');
</style>
