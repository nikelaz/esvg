import { optimize, OptimizeOptions, Plugin } from 'svgo';
import generateHash from './generate-hash';
import SVG from './svg';

/**
 * Default options for the svgo plugin
 * Reference: https://www.npmjs.com/package/svgo
 */
const defaultOptions: OptimizeOptions = {
  plugins: [
    {
      name: 'preset-default',
      params: {
        overrides: {
          removeViewBox: false,
        },
      },
    },
    { name: 'sortAttrs' },
    { name: 'removeDimensions' },
    { name: 'convertStyleToAttrs' }
  ]
};

class SVGOAdapter {
  static optimize(inputSVG: SVG, prefix: Boolean, pretty: Boolean = false): SVG {
    const options: OptimizeOptions = defaultOptions;

    if (prefix) {
      const prefixIdsPlugin: Plugin = {
        name: 'prefixIds',
        params: {
          prefix: generateHash()
        }
      };

      options.plugins.push(prefixIdsPlugin);
    }

    if (pretty) {
      options.js2svg = {
        pretty: true,
        indent: 2
      };
    }

    const result: any = optimize(inputSVG.toString(), options);

    if (result.error) throw new Error(result.error.toString());

    if (!result.data) throw new Error('No result data from the SVGO optimization');

    return new SVG(result.data);
  }
}

export default SVGOAdapter;
