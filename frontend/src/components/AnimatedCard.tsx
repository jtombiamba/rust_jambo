import React, { forwardRef } from 'react';
import { motion } from 'framer-motion';
import Card from './Card';

export interface AnimatedCardProps {
  index: number;
  faceUp?: boolean;
  onClick?: () => void;
  selected?: boolean;
  layoutId?: string;
  className?: string;
  style?: React.CSSProperties;
}

const AnimatedCard = forwardRef<HTMLDivElement, AnimatedCardProps>(({
  index,
  faceUp = true,
  onClick,
  selected = false,
  layoutId,
  className = '',
  style,
}, ref) => {
  return (
    <motion.div
      ref={ref}
      layoutId={layoutId}
      className={className}
      style={style}
      initial={{ opacity: 0 }}
      animate={{
        opacity: 1,
        scale: 1,
        rotate: 0,
      }}
      exit={{
        scale: 0.8,
        opacity: 0,
        transition: { duration: 0.2 },
      }}
      transition={{
        type: 'spring',
        stiffness: 300,
        damping: 25,
      }}
      whileHover={{ scale: 1.05 }}
      whileTap={{ scale: 0.95 }}
    >
      <Card
        index={index}
        faceUp={faceUp}
        onClick={onClick}
        selected={selected}
      />
    </motion.div>
  );
});

AnimatedCard.displayName = 'AnimatedCard';

export default AnimatedCard;
