import { optimize, OptimizeOptions, Plugin } from 'svgo';
import generateHash from './generate-hash';

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
    { name: 'removeDimensions' }
  ]
};

class SVGOAdapter {
  static optimize(inputSVG: string): string {
    const options: OptimizeOptions = defaultOptions;

    const prefixIdsPlugin: Plugin = {
      name: 'prefixIds',
      params: {
        prefix: generateHash()
      }
    };

    options.plugins.push(prefixIdsPlugin);

    const result: any = optimize(inputSVG, options);

    return result.data;
  }
}

export default SVGOAdapter;
