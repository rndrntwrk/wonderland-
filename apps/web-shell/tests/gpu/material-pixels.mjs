// Identify intentionally pure red/blue markers in the synthetic material fixture.
// A ratio against only blue admits brown terrain as red; require dominance over
// both other channels. This tightens sample selection, not rendering thresholds.
export function materialPixel(image,x,y,channel){
  const p=(y*image.width+x)*4,value=image.pixels[p+channel];
  return value>100&&value>2*image.pixels[p+(channel+1)%3]&&value>2*image.pixels[p+(channel+2)%3];
}
