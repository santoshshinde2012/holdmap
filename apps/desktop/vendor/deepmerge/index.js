"use strict";
// deepmerge's API (deepmerge 4.3.1: `deepmerge(x, y, options)` and `deepmerge.all`), written for
// holdmap without its prototype poisoning (GHSA-gcqx-w48m-w2h4, no upstream fix): a merged
// object's keys are defined with Object.defineProperty, so no key can set its prototype, and an
// own "__proto__" key is never copied. Inherited keys are never merged, as upstream. Only the
// desktop build tooling (@sveltejs/vite-plugin-svelte) uses it; see vendor/README.md.

const REACT_ELEMENT = typeof Symbol === "function" && Symbol.for ? Symbol.for("react.element") : 0xeac7;

function isMergeableObject(value) {
  if (!value || typeof value !== "object") return false;
  const tag = Object.prototype.toString.call(value);
  return tag !== "[object RegExp]" && tag !== "[object Date]" && value.$$typeof !== REACT_ELEMENT;
}

const emptyTarget = (value) => (Array.isArray(value) ? [] : {});

function cloneUnlessOtherwiseSpecified(value, options) {
  return options.clone !== false && options.isMergeableObject(value) ? deepmerge(emptyTarget(value), value, options) : value;
}

function defaultArrayMerge(target, source, options) {
  return target.concat(source).map((element) => cloneUnlessOtherwiseSpecified(element, options));
}

function mergeFunction(key, options) {
  if (!options.customMerge) return deepmerge;
  const custom = options.customMerge(key);
  return typeof custom === "function" ? custom : deepmerge;
}

/** Own enumerable keys and symbols, never "__proto__". */
function keys(object) {
  const symbols = Object.getOwnPropertySymbols(object).filter((symbol) => Object.prototype.propertyIsEnumerable.call(object, symbol));
  return [...Object.keys(object), ...symbols].filter((key) => key !== "__proto__");
}

const isOn = (object, key) => {
  try { return key in object; } catch { return false; }
};
/** On the target's prototype chain but not its own enumerable key: merging into it would reach the prototype. */
const isUnsafe = (target, key) => isOn(target, key) && !(Object.prototype.hasOwnProperty.call(target, key) && Object.prototype.propertyIsEnumerable.call(target, key));

function define(object, key, value) {
  Object.defineProperty(object, key, { value, enumerable: true, writable: true, configurable: true });
}

function mergeObject(target, source, options) {
  const destination = {};
  if (options.isMergeableObject(target)) for (const key of keys(target)) define(destination, key, cloneUnlessOtherwiseSpecified(target[key], options));
  for (const key of keys(source)) {
    if (isUnsafe(target, key)) continue;
    if (isOn(target, key) && options.isMergeableObject(source[key])) define(destination, key, mergeFunction(key, options)(target[key], source[key], options));
    else define(destination, key, cloneUnlessOtherwiseSpecified(source[key], options));
  }
  return destination;
}

function deepmerge(target, source, options) {
  const resolved = Object.assign({}, options, {
    arrayMerge: (options && options.arrayMerge) || defaultArrayMerge,
    isMergeableObject: (options && options.isMergeableObject) || isMergeableObject,
    cloneUnlessOtherwiseSpecified,
  });
  const sourceIsArray = Array.isArray(source);
  if (sourceIsArray !== Array.isArray(target)) return cloneUnlessOtherwiseSpecified(source, resolved);
  return sourceIsArray ? resolved.arrayMerge(target, source, resolved) : mergeObject(target, source, resolved);
}

deepmerge.all = function all(array, options) {
  if (!Array.isArray(array)) throw new Error("first argument should be an array");
  return array.reduce((merged, next) => deepmerge(merged, next, options), {});
};

module.exports = deepmerge;
